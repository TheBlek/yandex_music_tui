use crate::api::*;

use reqwest::{
    Client,
    Error,
};

use tokio::{
    task::JoinHandle,
    runtime::Handle,
    time::{
        Instant,
        Interval,
        Duration,
        interval_at,
    },
};

use rodio::{
    Sink,
    OutputStream,
    OutputStreamHandle,
    Decoder,
};

use rand::{
    Rng,
    seq::SliceRandom,
};
pub struct Player {
    account: AccountStatus,
    tracks: Vec<Track>,
    queue: Vec<usize>,
    queue_position: usize,
    music_sink: Sink,
    client: &'static Client,
    next_track_task_handle: Option<JoinHandle<Result<TrackData, Error>>>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    metronom: Interval,
}

pub async fn init_player(client: &'static Client, frame_time: u64) -> Result<Player, Error> {
    let account = account_status(&client).await?;
    let tracks = liked_music_tracks(account.uid, &client).await?;
    for track in &tracks {
        if track.duration.is_none() {
            println!("{:?}", track);
        }
    }

    let (stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();

    Ok(
        Player {
            account,
            queue: Vec::from_iter(0..tracks.len()),
            tracks,
            music_sink: sink,
            _stream: stream,
            stream_handle,
            queue_position: 0,
            next_track_task_handle: None,
            client,
            metronom: interval_at(Instant::now(), Duration::from_millis(frame_time)),
        }
    )
}

impl Player {
    pub fn next_track<'a>(&'a self) -> &'a Track {
        &self.tracks[self.queue[self.queue_position]]
    }

    pub fn track_after_n<'a>(&'a self, n: usize) -> &'a Track {
        &self.tracks[self.queue[self.queue_position + n]]
    }

    pub fn volume(&self) -> f32 {
        self.music_sink.volume()
    }

    pub fn speed(&self) -> f32 {
        self.music_sink.speed()
    }

    pub fn change_volume(&self, delta: f32) {
        self.music_sink.set_volume(self.music_sink.volume() + delta);
    }

    pub fn change_speed(&self, delta: f32) {
        self.music_sink.set_speed(self.music_sink.speed() + delta);
    }

    pub fn move_next(&mut self) {
        let (volume, speed) = (self.music_sink.volume(), self.music_sink.speed());
        self.music_sink.stop();

        self.music_sink = Sink::try_new(&self.stream_handle).unwrap();
        self.music_sink.set_volume(volume);
        self.music_sink.set_speed(speed);
    }

    pub fn move_prev(&mut self) {
        if self.queue_position > 1 {
            self.queue_position -= 2;

            self.next_track_task_handle = None;

            let (volume, speed) = (self.music_sink.volume(), self.music_sink.speed());
            self.music_sink.stop();

            self.music_sink = Sink::try_new(&self.stream_handle).unwrap();
            self.music_sink.set_volume(volume);
            self.music_sink.set_speed(speed);
        }
    }

    pub fn toggle_playback(&self) {
        if self.music_sink.is_paused() {
            self.music_sink.play();
        } else {
            self.music_sink.pause();
        }
    }

    pub fn shuffle_tracks(&mut self, rng: &mut impl Rng) {
        self.queue.shuffle(rng); 
        self.reset();
    }
    
    pub fn reset(&mut self) {
        self.queue_position = 0;
        self.next_track_task_handle = None;
    }
}

pub async fn playlists(player: &Player) -> Result<Vec<PlaylistInfo>, Error> {
    crate::api::playlists(player.account.uid, player.client).await
}

pub async fn load_playlist_into_player(player:&mut Player, playlist: &PlaylistInfo) -> Result<(), Error> {
   player.tracks = tracks_from_playlist(playlist, player.client).await?;
   player.reset();
   player.queue = Vec::from_iter(0..player.tracks.len());

   Ok(())
}

pub async fn load_favorites_into_player(player:&mut Player) -> Result<(), Error> {
   player.tracks = liked_music_tracks(player.account.uid, player.client).await?;
   player.reset();
   player.queue = Vec::from_iter(0..player.tracks.len());

   Ok(())
}

pub async fn update_player(player: &mut Player) {
    player.metronom.tick().await;

    if player.music_sink.empty() {
        let data = if let Some(handle) = player.next_track_task_handle.take() { 
            println!("Awaiting handle on the task"); 
            handle.await.unwrap().unwrap() 
        } else { 
            println!("Loading track directly!"); 
            let id = player.next_track().id;
            download_data(id, player.client)
                .await
                .unwrap()
        };
        println!("Playing: {}", player.next_track());
        
        player.music_sink.append(Decoder::new(data.data).unwrap());

        player.queue_position += 1; 
    } else if player.next_track_task_handle.is_none() {
        println!("Scheduling next track download");
        player.next_track_task_handle = Some(
            Handle::current().spawn(
                    download_data(player.next_track().id, player.client)
                )
        );
    }
}
