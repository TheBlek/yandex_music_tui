use reqwest::{
    Client,
    Error,
    header,
};
use std::io::Cursor;
use serde::{
    Deserialize,
    Deserializer,
    de,
};

#[derive(Debug, Deserialize)]
pub struct AccountStatus {
    uid: u64,
    #[serde(rename = "displayName")]
    display_name: String,
    login: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TrackInfo {
    #[serde(deserialize_with = "u64_from_str")]
    pub id: TrackID,
    #[serde(rename = "albumId", deserialize_with = "u64_from_str")]
    album_id: u64,
}

type TrackID = u64;

#[derive(Debug, Deserialize)]
pub struct Track {
    #[serde(deserialize_with="u64_from_str")]
    pub id: TrackID,
    pub title: String,
    pub major: Option<Major>,
    pub albums: Vec<AlbumInfo>,
    pub artists: Vec<ArtistInfo>,
    #[serde(rename = "durationMs")]
    pub duration: Option<u64>,
}

impl std::fmt::Display for Track {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.title, Artists(&self.artists))
    }
}

#[derive(PartialEq, Debug, Deserialize)]
pub enum AlbumType {
    #[serde(rename="music")]
    Music,
    #[serde(rename="podcast")]
    Podcast,
}

#[derive(Debug, Deserialize)]
pub struct AlbumInfo {
    id: u64,
    title: String,
    #[serde(rename="metaType")]
    meta_type: AlbumType,
    #[serde(rename="trackCount")]
    track_count: u32,
    #[serde(rename="likesCount")]
    likes_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistInfo {
    id: u64,
    name: String,
}

impl std::fmt::Display for ArtistInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub struct Artists<'a> (pub &'a Vec<ArtistInfo>);

impl std::ops::Deref for Artists<'_> {
    type Target = Vec<ArtistInfo>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl std::fmt::Display for Artists<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for i in 0..self.len() - 1 {
            write!(f, "{}, ", self[i])?; 
        }
        write!(f, "{}", self[self.len() - 1])?;
        write!(f, ")")
    }
}

#[derive(Debug, Deserialize)]
pub struct DownloadInfo {
    codec: Codec,
    #[serde(rename="downloadInfoUrl")]
    url: String,
    #[serde(rename="bitrateInKbps")]
    bitrate: u32,
}

#[derive(Debug)]
pub struct TrackData {
    pub id: TrackID,
    pub loaded: std::time::Instant,
    pub data: Cursor<bytes::Bytes>,
}

#[derive(Debug, Deserialize)]
pub enum Codec {
    #[serde(rename="mp3")]
    MP3,
    #[serde(rename="aac")]
    AAC,
}

#[derive(Debug, Deserialize)]
pub struct Major {
    id: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct AccountStatusResponse {
    result: AccountStatusResponseResult,
}

#[derive(Debug, Deserialize)]
struct AccountStatusResponseResult {
    account: AccountStatus,
}

#[derive(Debug, Deserialize)]
struct TracksInfoResponse {
    result: TracksInfoResponseResult,
}

#[derive(Debug, Deserialize)]
struct TracksInfoResponseResult {
    library: TracksInfoLibrary,
}

#[derive(Debug, Deserialize)]
struct TracksInfoLibrary {
    tracks: Vec<TrackInfo>,
}

fn u64_from_str<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where D: Deserializer<'de>
{
    let s = String::deserialize(deserializer)?;
    u64::from_str_radix(&s, 10).map_err(de::Error::custom)
}

#[derive(Debug, Deserialize)]
struct TrackQueryResponse {
    result: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct DownloadInfoResponse {
    result: Vec<DownloadInfo>,
}


pub fn authorized_client(token: &str) -> Result<Client, Error> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        "Authorization",
        header::HeaderValue::from_str(
            &(format!("OAuth {}", token))
        ).unwrap()
    );

    Ok(
        Client::builder()
            .default_headers(headers)
            .build()?
    )
}


pub async fn account_uid(client: &Client) -> Result<u64, Error> {
    Ok(
        client
            .get("https://api.music.yandex.net/account/status/")
            .send()
            .await?
            .json::<AccountStatusResponse>()
            .await?
            .result
            .account
            .uid
    )
}

pub async fn liked_tracks_infos(uid: u64, client: &Client) -> Result<Vec<TrackInfo>, Error> {
    Ok(
        client
            .get(format!("https://api.music.yandex.net/users/{}/likes/tracks", uid))
            .send()
            .await?
            .json::<TracksInfoResponse>()
            .await?
            .result
            .library
            .tracks
    )
}

pub async fn fetch_track(track_id: TrackID, client: &Client, attempts: Option<usize>) -> Result<Track, Error> {
    let mut left = attempts.unwrap_or(1);
    let mut error = None;
    while left > 0 {
        match client
                .post("https://api.music.yandex.net/tracks")
                .query(&[("trackIds", track_id)])
                .send()
                .await
        {

            Ok(resp) => {
                return 
                    Ok(
                        resp
                            .json::<TrackQueryResponse>()
                            .await
                            .unwrap()
                            .result
                            .into_iter()
                            .next()
                            .unwrap()
                    )
            }
            Err(err) => {
                left -= 1;
                error = Some(err);
                continue;
            }
        }
    }
    return Err(error.unwrap());
}

pub async fn liked_tracks(uid: u64, client: &Client) -> Result<Vec<Track>, Error> {
    let infos = liked_tracks_infos(uid, client).await?;

    Ok(
        futures::future::join_all(
            infos
                .iter()
                .map(|info| fetch_track(info.id, client, Some(2)))
        )
        .await
        .into_iter()
        .filter_map(|track_res| track_res.ok())
        .collect()
    )
}

pub async fn liked_music_tracks(uid: u64, client: &Client) -> Result<Vec<Track>, Error> {
    Ok(
        liked_tracks(uid, client)
            .await?
            .into_iter()
            .filter(|track| track.albums[0].meta_type == AlbumType::Music)
            .collect()
    )
}

async fn direct_link(info: &DownloadInfo, client: &Client) -> Result<String, Error> {
    let bytes = client
        .get(&info.url)
        .send()
        .await?
        .bytes()
        .await?;
    
    let xml = xmltree::Element::parse(&*bytes).unwrap();

    let host = xml.get_child("host").unwrap().get_text().unwrap();
    let path = xml.get_child("path").unwrap().get_text().unwrap();
    let s = xml.get_child("s").unwrap().get_text().unwrap();
    let ts = xml.get_child("ts").unwrap().get_text().unwrap();
    let sign = hex::encode::<[u8; 16]>(
        md5::compute(
            ("XGRlBW9FXlekgbPrRHuSiA".to_owned() + &path[1..] + &s).as_bytes()
        ).into()
    );

    Ok(
        format!(
            "https://{}/get-mp3/{}/{}{}",
            host,
            sign,
            ts,
            path
        )
    )
}

pub async fn download_data(id: TrackID, client: &Client) -> Result<TrackData, Error> {
    let infos = client
        .get(format!("https://api.music.yandex.net/tracks/{}/download-info", id))
        .send()
        .await?
        .json::<DownloadInfoResponse>()
        .await?
        .result;

    let link = direct_link(&infos[0], client).await?;
    let bytes = client
        .get(link)
        .send()
        .await?
        .bytes()
        .await?;

    Ok(
        TrackData {
            id,
            data: std::io::Cursor::new(bytes),
            loaded: std::time::Instant::now(),
        }
    )
}

#[derive(Debug, Deserialize)]
struct PlaylistsResponse {
   result: Vec<PlaylistInfo>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistInfo {
    pub title: String,
    #[serde(rename = "trackCount")]
    pub track_count: usize,
    pub kind: usize,
    pub uid: usize,
}


pub async fn playlists(uid: u64, client: &Client) -> Result<Vec<PlaylistInfo>, Error> {
    Ok(
        client
            .get(format!("https://api.music.yandex.net/users/{}/playlists/list", uid))
            .send()
            .await?
            .json::<PlaylistsResponse>()
            .await?
            .result
    )
}

#[derive(Debug, Deserialize)]
struct PlaylistTracksResponse {
    result: PlaylistTracksResponseResult,
}

#[derive(Debug, Deserialize)]
struct PlaylistTracksResponseResult {
    tracks: Vec<TrackWrapper>,
}

#[derive(Debug, Deserialize)]
struct TrackWrapper {
    track: Track,
}

pub async fn tracks_from_playlist(info: &PlaylistInfo, client: &Client) -> Result<Vec<Track>, Error> {
    Ok(
        client
            .get(format!("https://api.music.yandex.net/users/{}/playlists/{}", info.uid, info.kind))
            .send()
            .await?
            .json::<PlaylistTracksResponse>()
            .await?
            .result
            .tracks
            .into_iter()
            .map(|wrapper| wrapper.track)
            .collect::<Vec<Track>>()
    )
}
