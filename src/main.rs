mod api;
use api::{
    Track,
    TrackData,
    download_data,
};
use reqwest::{
    Client,
    Error,
};
use std::sync::mpsc;
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
    thread_rng,
};

struct FavoritesPlayer {
    favorites: Vec<Track>,
    queue: Vec<usize>,
    queue_position: usize,
    music_sink: Sink,
    client: &'static Client,
    next_track_task_handle: Option<JoinHandle<Result<TrackData, Error>>>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    metronom: Interval,
}

async fn init_player(client: &'static Client, frame_time: u64) -> Result<FavoritesPlayer, Error> {
    use api::*;

    let uid = account_uid(&client).await?;
    let tracks = liked_music_tracks(uid, &client).await?;
    for track in &tracks {
        if track.duration.is_none() {
            println!("{:?}", track);
        }
    }

    let (stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();

    Ok(
        FavoritesPlayer {
            queue: Vec::from_iter(0..tracks.len()),
            favorites: tracks,
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

impl FavoritesPlayer {
    fn next_track<'a>(&'a self) -> &'a Track {
        &self.favorites[self.queue[self.queue_position]]
    }

    fn track_after_n<'a>(&'a self, n: usize) -> &'a Track {
        &self.favorites[self.queue[self.queue_position + n]]
    }

    fn volume(&self) -> f32 {
        self.music_sink.volume()
    }

    fn speed(&self) -> f32 {
        self.music_sink.speed()
    }

    fn change_volume(&self, delta: f32) {
        self.music_sink.set_volume(self.music_sink.volume() + delta);
    }

    fn change_speed(&self, delta: f32) {
        self.music_sink.set_speed(self.music_sink.speed() + delta);
    }

    fn move_next(&mut self) {
        let (volume, speed) = (self.music_sink.volume(), self.music_sink.speed());
        self.music_sink.stop();

        self.music_sink = Sink::try_new(&self.stream_handle).unwrap();
        self.music_sink.set_volume(volume);
        self.music_sink.set_speed(speed);
    }

    fn move_prev(&mut self) {
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

    fn toggle_playback(&self) {
        if self.music_sink.is_paused() {
            self.music_sink.play();
        } else {
            self.music_sink.pause();
        }
    }

    fn shuffle_tracks(&mut self, rng: &mut impl Rng) {
        self.queue.shuffle(rng); 
        self.queue_position = 0;
        self.next_track_task_handle = None;
    }
}

async fn update_player(player: &mut FavoritesPlayer) {
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

enum AppEvent {
    ChangeVolume(f32),
    SetVolume(f32),
    PrintVolume,
    ChangeSpeed(f32),
    PrintSpeed,
    SetSpeed(f32),
    TogglePlayback,
    NextTrack,
    PrevTrack,
    Shuffle,
    Quit,
}

lazy_static::lazy_static!{
    static ref CLIENT: Client = api::authorized_client(
        "y0_AgAAAAAVQHDFAAG8XgAAAADNLVcPViQQUTqtR66OJ5F0Db_M64fmFFQ"
    ).expect("Failed to create an authorised client");
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel();

    let handle = Handle::current();

    let io_handle = handle.spawn(async move {
        let error = |message| {
            println!("Error parsing input: {}", message);
        };
        loop {
            let mut input = String::new();

            std::io::stdin().read_line(&mut input).expect("error with stdin");
            let mut args = input.trim_end().split_whitespace();
            let command = args.next().unwrap_or("err");
            match command {
                "vu" => {tx.send(AppEvent::ChangeVolume(0.05)).unwrap()},
                "vd" => {tx.send(AppEvent::ChangeVolume(-0.05)).unwrap()},
                "vg" => {tx.send(AppEvent::PrintVolume).unwrap()},
                "vs" => 'vs : {
                    let Some(string) = args.next() else {
                        error("Not enough arguments supplied"); 
                        break 'vs;
                    };
                    let Ok(value) = string.parse::<f32>() else {
                        error("Invalid argument format");
                        break 'vs;
                    };
                    tx.send(AppEvent::SetVolume(value)).unwrap()
                },
                "su" => {tx.send(AppEvent::ChangeSpeed(0.5)).unwrap()},
                "sd" => {tx.send(AppEvent::ChangeSpeed(-0.5)).unwrap()},
                "sg" => {tx.send(AppEvent::PrintSpeed).unwrap()},
                "ss" => 'ss : {
                    let Some(string) = args.next() else {
                        error("Not enough arguments supplied"); 
                        break 'ss;
                    };
                    let Ok(value) = string.parse::<f32>() else {
                        error("Invalid argument format");
                        break 'ss;
                    };
                    tx.send(AppEvent::SetSpeed(value)).unwrap()
                },
                "p" => {tx.send(AppEvent::TogglePlayback).unwrap()},
                "next" => {tx.send(AppEvent::NextTrack).unwrap()},
                "prev" => {tx.send(AppEvent::PrevTrack).unwrap()},
                "sh" => {tx.send(AppEvent::Shuffle).unwrap()},
                "q" => {
                    tx.send(AppEvent::Quit).unwrap();
                    break;
                },
                _ => {
                    error("Unknown command");
                },
            };
        }
    });
    
    let mut player = init_player(&CLIENT, 100).await.unwrap();
    let mut rng = thread_rng();
    'app: loop {
        update_player(&mut player).await;
        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::ChangeVolume(volume) => { player.change_volume(volume) },
                AppEvent::SetVolume(volume) => { player.change_volume(volume - player.volume()) },
                AppEvent::PrintVolume => { println!("Current volume: {}", player.volume()) },
                AppEvent::ChangeSpeed(speed) => { player.change_speed(speed) },
                AppEvent::SetSpeed(speed) => { player.change_speed(speed - player.speed()) },
                AppEvent::PrintSpeed => { println!("Current speed: {}", player.speed()) },
                AppEvent::TogglePlayback => { player.toggle_playback() },
                AppEvent::Quit => { break 'app },
                AppEvent::NextTrack => { player.move_next() },
                AppEvent::PrevTrack => { player.move_prev() },
                AppEvent::Shuffle => { player.shuffle_tracks(&mut rng) },
            }
        }
    }

    io_handle.await.unwrap();
}
