mod api;
mod player;

use std::sync::mpsc;
use player::*;
use tokio::runtime::Handle;
use reqwest::Client;

use rand::thread_rng;


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
    ListPlaylists,
    LoadPlaylist(u32),
    LoadFavorites,
    Quit,
}

lazy_static::lazy_static! {
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
                "playlists" => {tx.send(AppEvent::ListPlaylists).unwrap()},
                "load-playlist" => 'ss : {
                    let Some(string) = args.next() else {
                        error("Not enough arguments supplied"); 
                        break 'ss;
                    };
                    let Ok(value) = string.parse::<u32>() else {
                        error("Invalid argument format");
                        break 'ss;
                    };
                    tx.send(AppEvent::LoadPlaylist(value)).unwrap()
                },
                "load-favorites" => {tx.send(AppEvent::LoadFavorites).unwrap()},
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
                AppEvent::NextTrack => { player.move_next() },
                AppEvent::PrevTrack => { player.move_prev() },
                AppEvent::ListPlaylists => {
                    let playlists = playlists(&player)
                        .await
                        .unwrap();
                    for (n, playlist) in playlists.into_iter().enumerate() {
                        println!("{}. {}", n, playlist.title);
                    }
                },
                AppEvent::LoadPlaylist(n) => { 
                    let playlists = playlists(&player)
                        .await
                        .unwrap();
                    println!("Loading {}", playlists[n as usize].title);
                    if let Err(_) = load_playlist_into_player(&mut player, &playlists[n as usize]).await {
                        break 'app;
                    }
                },
                AppEvent::LoadFavorites => { 
                    load_favorites_into_player(&mut player).await.unwrap()
                },
                AppEvent::Shuffle => { player.shuffle_tracks(&mut rng) },
                AppEvent::Quit => { break 'app },
            }
        }
    }

    io_handle.await.unwrap();
}
