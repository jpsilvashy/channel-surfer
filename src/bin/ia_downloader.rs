use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
// Standard ANSI color codes
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use futures_util::StreamExt;

#[derive(Parser)]
#[command(name = "ia-downloader")]
#[command(about = "Search and download videos from Internet Archive", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search for videos on Internet Archive
    Search {
        /// Search query
        #[arg(required = true)]
        query: String,

        /// Number of results to return (max 100)
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Filter by media type (video, movies, etc.)
        #[arg(short, long, default_value = "movies")]
        media_type: String,
    },
    /// Download a video from Internet Archive by identifier
    Download {
        /// Internet Archive identifier
        #[arg(required = true)]
        identifier: String,

        /// Output directory (default: ./videos)
        #[arg(short, long, default_value = "./videos")]
        output_dir: PathBuf,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct SearchResponse {
    response: SearchResponseInner,
}

#[derive(Serialize, Deserialize, Debug)]
struct SearchResponseInner {
    numFound: usize,
    start: usize,
    docs: Vec<Document>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Document {
    identifier: String,
    title: Option<String>,
    description: Option<String>,
    mediatype: Option<String>,
    year: Option<String>,
    creator: Option<Vec<String>>,
    subject: Option<Vec<String>>,
    item_size: Option<usize>,
    downloads: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
struct MetadataResponse {
    files: Vec<FileInfo>,
    metadata: Metadata,
}

#[derive(Serialize, Deserialize, Debug)]
struct FileInfo {
    name: String,
    format: Option<String>,
    size: Option<String>,
    source: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Metadata {
    identifier: String,
    title: Option<String>,
    description: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new();

    match cli.command {
        Commands::Search {
            query,
            limit,
            media_type,
        } => {
            search_videos(&client, &query, limit, &media_type).await?;
        }
        Commands::Download {
            identifier,
            output_dir,
        } => {
            download_video(&client, &identifier, &output_dir).await?;
        }
    }

    Ok(())
}

async fn search_videos(client: &Client, query: &str, limit: usize, media_type: &str) -> Result<()> {
    println!("🔍 Searching for: {}", query);

    let url = format!(
        "https://archive.org/advancedsearch.php?q=mediatype%3A{media_type}+AND+{query}&fl[]=identifier,title,description,mediatype,year,creator,subject,item_size,downloads&sort[]=downloads+desc&rows={limit}&page=1&output=json",
        media_type = media_type,
        query = query.replace(" ", "+"),
        limit = limit
    );

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to send search request")?;

    let search_result: SearchResponse = response
        .json()
        .await
        .context("Failed to parse search results")?;

    if search_result.response.docs.is_empty() {
        println!("No results found for query: {}", query);
        return Ok(());
    }

    println!(
        "📊 Found {} results (showing {})",
        search_result.response.numFound,
        search_result.response.docs.len()
    );
    println!("{}", "=".repeat(80));

    for (i, doc) in search_result.response.docs.iter().enumerate() {
        let title = doc.title.as_deref().unwrap_or("(No Title)");
        let year = doc.year.as_deref().unwrap_or("Unknown Year");
        let creators = match &doc.creator {
            Some(creators) if !creators.is_empty() => creators.join(", "),
            _ => "Unknown".to_string(),
        };
        let downloads = doc.downloads.unwrap_or(0);

        // ANSI color codes
        const GOLD: &str = "\x1b[33;1m";
        const WHITE: &str = "\x1b[37;1m";
        const GRAY: &str = "\x1b[37m";
        const GREEN: &str = "\x1b[32m";
        const RESET: &str = "\x1b[0m";

        println!(
            "{}{}{} {}{}{} ({})",
            GOLD, format!("[{}]", i + 1), RESET,
            WHITE, title, RESET,
            year
        );
        println!("   Creator: {}{}{}", GRAY, creators, RESET);
        println!("   ID: {}{}{}", GREEN, doc.identifier, RESET);
        println!("   Downloads: {}{}{}", GRAY, downloads, RESET);

        if let Some(description) = &doc.description {
            // Truncate description if too long
            let desc = if description.len() > 200 {
                format!("{}...", &description[..200])
            } else {
                description.clone()
            };
            println!("   Description: {}{}{}", GRAY, desc, RESET);
        }

        println!("{}", "-".repeat(80));
    }

    println!();
    println!("To download a video, run:");
    println!("cargo run --bin ia-downloader download <identifier>");
    println!();

    Ok(())
}

async fn download_video(client: &Client, identifier: &str, output_dir: &Path) -> Result<()> {
    println!("📝 Getting metadata for: {}", identifier);

    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).context("Failed to create output directory")?;
    }

    // Fetch item metadata to get file list
    let metadata_url = format!("https://archive.org/metadata/{}", identifier);
    let response = client
        .get(&metadata_url)
        .send()
        .await
        .context("Failed to fetch metadata")?;

    let metadata: MetadataResponse = response
        .json()
        .await
        .context("Failed to parse metadata")?;

    println!("📦 Found {} files", metadata.files.len());
    
    // Filter out video files
    let video_files: Vec<&FileInfo> = metadata
        .files
        .iter()
        .filter(|file| {
            if let Some(format) = &file.format {
                format.contains("MPEG") || format.contains("MP4") || format.contains("AVI") || 
                format.contains("QuickTime") || format.contains("Matroska") || format.contains("WebM")
            } else {
                // If format is not specified, check file extension
                file.name.ends_with(".mp4") || file.name.ends_with(".avi") || 
                file.name.ends_with(".mkv") || file.name.ends_with(".mov") ||
                file.name.ends_with(".webm") || file.name.ends_with(".flv")
            }
        })
        .collect();

    if video_files.is_empty() {
        println!("❌ No video files found for item: {}", identifier);
        return Ok(());
    }

    println!("🎬 Found {} video files:", video_files.len());
    for (i, file) in video_files.iter().enumerate() {
        let size = file.size.as_deref().unwrap_or("Unknown size");
        println!("  [{}] {} ({})", i + 1, file.name, size);
    }

    // If there are multiple video files, prompt user to select one
    let selected_file = if video_files.len() == 1 {
        video_files[0]
    } else {
        println!("\nEnter the number of the file to download [1-{}]:", video_files.len());
        
        // In a real application, we'd get user input here
        // For this example, we'll just take the first file
        println!("Auto-selecting the first video file");
        video_files[0]
    };

    // Create a filename with the identifier for context
    let original_name = &selected_file.name;
    // Extract extension
    let extension = Path::new(original_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("mp4");
    
    // Clean up the title to use as filename
    let title = metadata
        .metadata
        .title
        .as_ref()
        .map(|t| {
            // Replace invalid filename characters with a simple approach
            t.replace('/', "_")
             .replace('\\', "_")
             .replace('?', "_")
             .replace(':', "_")
             .replace('*', "_")
             .replace('"', "_")
             .replace('<', "_")
             .replace('>', "_")
             .replace('|', "_")
        })
        .unwrap_or_else(|| identifier.to_string());

    // Create filename with identifier suffix for uniqueness
    let filename = format!("{}, {}.ia.{}", title, identifier, extension);
    let output_path = output_dir.join(&filename);

    // Download the file
    println!("📥 Downloading: {}", filename);
    println!("   to: {}", output_path.display());

    let download_url = format!(
        "https://archive.org/download/{}/{}",
        identifier, selected_file.name
    );

    let response = client
        .get(&download_url)
        .send()
        .await
        .context("Failed to start download")?;

    let total_size = response
        .content_length()
        .unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Create the file
    let mut file = File::create(&output_path).context("Failed to create output file")?;
    let mut downloaded = 0;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.context("Error while downloading file")?;
        file.write_all(&chunk).context("Error while writing to file")?;
        let new = downloaded + chunk.len() as u64;
        downloaded = new;
        pb.set_position(new);
    }

    pb.finish_with_message("Download complete!");
    
    println!("\n✅ Downloaded: {}", filename);
    println!("   Saved to: {}", output_path.display());

    Ok(())
}
