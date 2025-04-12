use warp::Filter;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::fs;
use warp::sse::Event;
use tokio_stream::wrappers::BroadcastStream;
use futures::StreamExt;
use tokio::sync::broadcast;

#[derive(Deserialize, Serialize)]
struct Video {
    filename: String,
}

#[tokio::main]
async fn main() {
    println!("Starting server...");

    // Read the list of videos from the "videos" directory
    let video_dir = "./videos";
    let videos = read_videos_from_directory(video_dir).unwrap_or_else(|err| {
        eprintln!("Error reading videos directory: {}", err);
        vec![]
    });
    let video_list = Arc::new(Mutex::new(videos));

    // Create a broadcast channel for SSE
    let (tx, _) = broadcast::channel(10);
    let tx_filter = warp::any().map(move || tx.clone());

    let video_list_filter = warp::any().map(move || Arc::clone(&video_list));

    let list_videos = warp::path("videos")
        .and(warp::get())
        .and(video_list_filter.clone())
        .and_then(|videos: Arc<Mutex<Vec<String>>>| async move {
            let videos = videos.lock().await;
            println!("Listing videos...");
            Ok::<_, warp::Rejection>(warp::reply::json(&*videos))
        });

    let play_video = warp::path("play")
        .and(warp::post())
        .and(warp::body::json())
        .and(video_list_filter.clone())
        .and(tx_filter.clone())
        .and_then(move |video: Video, _videos: Arc<Mutex<Vec<String>>>, tx: broadcast::Sender<String>| {
            let video_dir = video_dir.to_string();
            async move {
                let filename = format!("{}/{}", video_dir, video.filename); // Include directory path
                println!("Playing video: {}", filename);

                let tx_clone = tx.clone();
                let video_filename = video.filename.clone();
                tokio::spawn(async move {
                    let output = Command::new("mpv")
                        .arg(&filename) // Use the filename with the directory path
                        .output()
                        .expect("failed to execute process");

                    println!("{:?}", output);

                    tx_clone.send(video_filename).unwrap();
                });

                Ok::<_, warp::Rejection>(warp::reply::json(&video))
            }
        });

    let sse_video = warp::path("sse")
        .and(warp::get())
        .and(tx_filter)
        .map(|tx: broadcast::Sender<String>| {
            let stream = BroadcastStream::new(tx.subscribe()).filter_map(|result| async {
                match result {
                    Ok(msg) => Some(Ok::<Event, warp::Error>(Event::default().data(msg))),
                    Err(_) => None,
                }
            });
            warp::sse::reply(warp::sse::keep_alive().stream(stream))
        });

    let static_files = warp::path::end()
        .and(warp::fs::dir("./static"));

    let routes = list_videos.or(play_video).or(sse_video).or(static_files);

    println!("Server running on http://localhost:3030");

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

fn read_videos_from_directory(dir: &str) -> Result<Vec<String>, std::io::Error> {
    let mut videos = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(extension) = path.extension() {
                if extension == "mp4" || extension == "avi" || extension == "mkv" {
                    if let Some(filename) = path.file_name() {
                        if let Some(filename_str) = filename.to_str() {
                            videos.push(filename_str.to_string());
                        }
                    }
                }
            }
        }
    }
    Ok(videos)
}
