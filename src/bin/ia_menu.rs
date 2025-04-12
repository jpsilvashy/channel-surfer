use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
// No fancy terminal handling, just simple IO

// Data structures for Internet Archive API - made more flexible for varying API responses
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum SearchResponse {
    Success { response: SearchResponseInner },
    Error { error: String },
}

#[derive(Serialize, Deserialize, Debug)]
struct SearchResponseInner {
    #[serde(rename = "numFound")]
    num_found: usize,
    start: usize,
    docs: Vec<Document>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Document {
    identifier: String,
    title: Option<String>,
    description: Option<String>,
    mediatype: Option<String>,
    #[serde(deserialize_with = "deserialize_string_or_int")]
    year: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    creator: Vec<String>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    subject: Vec<String>,
    item_size: Option<usize>,
    downloads: Option<usize>,
}

// Custom deserializer to handle both String and Vec<String> cases
fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![value])
        }

        fn visit_seq<S>(self, visitor: S) -> Result<Self::Value, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(visitor))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            Deserialize::deserialize(deserializer)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

// Custom deserializer to handle both String and integer values for the year field
fn deserialize_string_or_int<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrInt;

    impl<'de> serde::de::Visitor<'de> for StringOrInt {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or integer")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            Deserialize::deserialize(deserializer)
        }
    }

    deserializer.deserialize_any(StringOrInt)
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
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_size")]
    size: Option<u64>,
    source: Option<String>,
    // Optional length/duration field
    runtime: Option<String>,
    length: Option<String>, // Alternative field for duration
}

#[derive(Serialize, Deserialize, Debug)]
struct Metadata {
    identifier: String,
    title: Option<String>,
    year: Option<String>,
    description: Option<String>,
    creator: Option<String>,
    subject: Option<String>,
    collection: Option<String>,
    // Additional fields that might be useful
    date: Option<String>,
    coverage: Option<String>,
}

// State tracking for downloads
struct DownloadState {
    active_downloads: HashMap<String, JoinHandle<Result<()>>>,
}

impl DownloadState {
    fn new() -> Self {
        Self {
            active_downloads: HashMap::new(),
        }
    }

    async fn add_download(&mut self, identifier: String, handle: JoinHandle<Result<()>>) {
        self.active_downloads.insert(identifier, handle);
    }

    async fn check_downloads(&mut self) {
        let mut completed = Vec::new();
        
        for (id, handle) in &self.active_downloads {
            if handle.is_finished() {
                completed.push(id.clone());
            }
        }

        for id in completed {
            if let Some(handle) = self.active_downloads.remove(&id) {
                match handle.await {
                    Ok(Ok(())) => println!("✓ Download completed: {}", id),
                    Ok(Err(e)) => println!("✗ Download failed for {}: {}", id, e),
                    Err(e) => println!("✗ Download task failed for {}: {}", id, e),
                }
            }
        }
    }

    fn has_active_downloads(&self) -> bool {
        !self.active_downloads.is_empty()
    }

    fn active_download_count(&self) -> usize {
        self.active_downloads.len()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create videos directory if it doesn't exist
    let videos_dir = "./videos";
    fs::create_dir_all(videos_dir)?;
    
    let client = Client::new();
    let download_state = Arc::new(Mutex::new(DownloadState::new()));
    
    // Main application loop
    run_simple_menu(&client, videos_dir, download_state).await
}

async fn run_simple_menu(client: &Client, videos_dir: &str, download_state: Arc<Mutex<DownloadState>>) -> Result<()> {
    loop {
        // Clear the screen with a simple method
        print!("\x1B[2J\x1B[1;1H"); // ANSI escape sequence to clear screen and move cursor to top-left
        io::stdout().flush()?;
        
        // Check if there are any completed downloads
        download_state.lock().await.check_downloads().await;
        
        // Show active downloads count
        let active_downloads = download_state.lock().await.active_download_count();
        if active_downloads > 0 {
            println!("📥 Active downloads: {}\n", active_downloads);
        }
        
        println!("🎬 Channel Surfer 🎬");
        println!("=====================================\n");
        
        // Simple numbered menu
        println!("1. Start TV guide server");
        println!("2. List local videos");
        println!("3. Search Internet Archive videos");
        println!("4. Clear all local videos");
        println!("5. Exit");
        
        if active_downloads > 0 {
            println!("6. Show download status");
        }
        
        print!("\nEnter your choice: ");
        io::stdout().flush()?;
        
        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;
        
        match choice.trim() {
            "1" => start_server().await?,
            "2" => list_local_videos(videos_dir).await?,
            "3" => search_and_download(client, videos_dir, Arc::clone(&download_state)).await?,
            "4" => clear_videos(videos_dir).await?,
            "5" => {
                if download_state.lock().await.has_active_downloads() {
                    print!("⚠️  You have active downloads. Are you sure you want to exit? (y/n): ");
                    io::stdout().flush()?;
                    
                    let mut confirm = String::new();
                    io::stdin().read_line(&mut confirm)?;
                    if confirm.trim().to_lowercase() == "y" {
                        println!("Exiting. Note: background downloads will be terminated.");
                        break;
                    }
                } else {
                    println!("Goodbye!");
                    break;
                }
            },
            "6" if active_downloads > 0 => {
                println!("\nCurrent active downloads:");
                let downloads = &download_state.lock().await.active_downloads;
                for (id, _) in downloads {
                    println!(" - {}", id);
                }
                
                println!("\nPress Enter to continue...");
                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer)?;
            },
            _ => println!("Invalid choice. Please try again."),
        }
    }
    
    Ok(())
}

async fn list_local_videos(videos_dir: &str) -> Result<()> {
    // Clear screen
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush()?;
    
    // Show TV Guide header with time
    let now = SystemTime::now();
    let time_since_epoch = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let current_hour = (time_since_epoch / 3600) % 24;
    let current_minute = (time_since_epoch / 60) % 60;
    
    // Build TV Guide themed header
    println!("\x1B[44m\x1B[33m"); // Blue background, yellow text (ANSI colors)
    println!("TV GUIDE{}{}:{} PM", " ".repeat(70), current_hour % 12, 
             if current_minute < 10 { format!("0{}", current_minute) } else { format!("{}", current_minute) });
    println!("\x1B[0m"); // Reset colors
    
    // Check for videos in the directory
    let entries = fs::read_dir(videos_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(extension) = entry.path().extension() {
                        extension.to_string_lossy().to_lowercase() == "mp4"
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        })
        .collect::<Vec<_>>();
    
    if entries.is_empty() {
        println!("\nNo videos found in {}\n", videos_dir);
    } else {
        // Organize videos by channel number
        let mut videos_by_channel: std::collections::BTreeMap<u8, Vec<TvGuideMetadata>> = std::collections::BTreeMap::new();
        
        // First pass: collect metadata
        for entry in entries {
            let path = entry.path();
            
            // Check for companion JSON metadata file
            let json_path = path.with_extension("json");
            if json_path.exists() {
                let json_content = fs::read_to_string(&json_path)?;
                if let Ok(metadata) = serde_json::from_str::<TvGuideMetadata>(&json_content) {
                    videos_by_channel.entry(metadata.channel_number)
                        .or_insert_with(Vec::new)
                        .push(metadata);
                }
            }
        }
        
        // Second pass: display in TV Guide format
        for (channel, videos) in &videos_by_channel {
            // Draw channel box with channel number and callsign
            if !videos.is_empty() {
                let callsign = &videos[0].station_callsign;
                
                // Draw the channel info in purple background (like the screenshot)
                println!("\x1B[45m\x1B[37m{: ^15}\x1B[0m", format!("CH {}", channel)); // Channel number
                println!("\x1B[45m\x1B[37m{: ^15}\x1B[0m", callsign); // Station callsign
                
                // Sort by start time
                let mut channel_videos = videos.clone();
                channel_videos.sort_by(|a, b| a.start_time.cmp(&b.start_time));
                
                // Display up to 3 programs for this channel
                for (i, video) in channel_videos.iter().take(3).enumerate() {
                    // Program start time & title (blue background)
                    println!("\x1B[44m\x1B[33m{: ^10}\x1B[0m \x1B[44m\x1B[33m{: <30}\x1B[0m", 
                             video.start_time, // Left box with time
                             video.title.chars().take(28).collect::<String>()); // Right box with title
                             
                    // Only show details for the first 2 entries to save space
                    if i < 2 {
                        println!("\x1B[44m\x1B[33m{: ^10}\x1B[0m", format!("{}m", video.duration.split_whitespace().next().unwrap_or("30")));
                    }
                }
                
                // Add a blank line between channels
                println!();
            }
        }
    }
    
    println!("\nPress Enter to return to the main menu...");
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;
    
    Ok(())
}

async fn clear_videos(videos_dir: &str) -> Result<()> {
    // Simple clear screen
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush()?;
    
    println!("⚠️  WARNING: This will delete all video files in {}.", videos_dir);
    println!("Are you sure you want to proceed? (y/n)");
    
    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm)?;
    
    if confirm.trim().to_lowercase() == "y" {
        let entries = fs::read_dir(videos_dir)?;
        let mut deleted_count = 0;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    let ext_str = extension.to_string_lossy().to_lowercase();
                    if ["mp4", "avi", "mkv", "mov", "webm", "flv"].contains(&ext_str.as_str()) {
                        fs::remove_file(&path)?;
                        deleted_count += 1;
                    }
                }
            }
        }
        
        println!("✓ Deleted {} video files.", deleted_count);
    } else {
        println!("Operation cancelled.");
    }
    
    prompt_user("\nPress Enter to return to the main menu...")?;
    
    Ok(())
}

async fn search_and_download(client: &Client, videos_dir: &str, download_state: Arc<Mutex<DownloadState>>) -> Result<()> {
    // Simple clear screen
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush()?;
    
    let query = prompt_user("Enter search keywords: ")?;
    let limit = prompt_user("Number of results to display (default 10): ")?;
    let limit = limit.trim().parse::<usize>().unwrap_or(10);
    
    println!("\n🔍 Searching for: {}", query);
    
    // Use a very simple search query for maximum compatibility
    let simplified_query = query.replace(" ", "+");
    
    // Use the basic search URL format
    let url = format!("https://archive.org/advancedsearch.php?q=mediatype:movies+{}&output=json&rows={}", 
                     simplified_query, limit);

    println!("Making request to: {}", url);
    
    // Make the API request and handle potential errors
    let response = match client.get(&url).send().await {
        Ok(response) => response,
        Err(e) => {
            println!("Error making search request: {}", e);
            prompt_user("\nPress Enter to return to the main menu...")?;
            return Ok(());
        }
    };

    // Get the response text and parse it
    let response_text = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            println!("Error reading response: {}", e);
            prompt_user("\nPress Enter to return to the main menu...")?;
            return Ok(());
        }
    };
    
    println!("\nResponse received. Let's search for videos using manual parsing.");
    
    // Skip the complex parsing and just extract identifiers from the JSON
    let docs = match try_extract_identifiers(&response_text) {
        Ok(docs) => docs,
        Err(e) => {
            println!("Error extracting video information: {}", e);
            println!("API Response: {}", &response_text.chars().take(200).collect::<String>());
            prompt_user("\nPress Enter to return to the main menu...")?;
            return Ok(());
        }
    };
    
    if docs.is_empty() {
        println!("No results found for query: {}", query);
        prompt_user("\nPress Enter to return to the main menu...")?;
        return Ok(());
    }
    
    println!("\n📋 Found {} results:", docs.len());

    for (i, doc) in docs.iter().enumerate() {
        let title = doc.title.as_deref().unwrap_or("(No Title)");
        let year = doc.year.as_deref().unwrap_or("Unknown");
        let creators = if !doc.creator.is_empty() {
            if doc.creator.len() > 1 {
                format!("{} et al", doc.creator[0])
            } else {
                doc.creator[0].clone()
            }
        } else {
            "Unknown".to_string()
        };
        
        // Format downloads with commas for readability
        let downloads = doc.downloads.unwrap_or(0);
        
        // Get the estimated file size if available
        let size_str = match doc.item_size {
            Some(size) => format_size(size),
            None => "~15MB (est.)".to_string() // Give an estimate instead of unknown
        };
        
        // Compact 2-line listing with all key info
        println!("[{}] {} ({}) {}", i + 1, title, year, size_str);
        println!("    Creator: {}  ID: {}  Downloads: {}", creators, doc.identifier, downloads);
        println!("{}", "-".repeat(60));
    }

    println!("\nEnter the number of the video to download (or press Enter to cancel): ");
    let mut selection = String::new();
    io::stdin().read_line(&mut selection)?;
    
    if selection.is_empty() {
        return Ok(());
    }
    
    let selected = match selection.trim().parse::<usize>() {
        Ok(num) if num > 0 && num <= docs.len() => num - 1,
        _ => {
            println!("Invalid selection.");
            prompt_user("\nPress Enter to return to the main menu...")?;
            return Ok(());
        }
    };

    // Get selected document and download it
    let selected_doc = &docs[selected];
    let identifier = selected_doc.identifier.clone();
    
    println!("Starting download for: {}", selected_doc.title.as_deref().unwrap_or(&identifier));
    
    // Clone what we need for the async block
    let client = client.clone();
    let videos_dir = videos_dir.to_string();
    let id_for_download = identifier.clone(); // Clone for async move
    
    // Start download in background
    let handle = tokio::spawn(async move {
        download_video(&client, &id_for_download, &videos_dir).await
    });
    
    // Register the download
    download_state.lock().await.add_download(identifier, handle).await;
    
    prompt_user("\nDownload started in background. Press Enter to return to the main menu...")?;
    
    // No terminal mode handling needed
    
    Ok(())
}

async fn download_video(client: &Client, identifier: &str, output_dir: &str) -> Result<()> {
    println!("📥 Fetching detailed metadata for {}...", identifier);
    
    // Fetch metadata for the video
    let metadata_url = format!("https://archive.org/metadata/{}", identifier);
    
    let response = client
        .get(&metadata_url)
        .send()
        .await
        .context("Failed to fetch metadata")?;
        
    // First get the raw JSON to diagnose issues if needed
    let raw_metadata = response.text().await?;
    
    // Try to parse it into our metadata structure
    let metadata_response: MetadataResponse = match serde_json::from_str(&raw_metadata) {
        Ok(metadata) => metadata,
        Err(e) => {
            println!("Warning: Error parsing metadata: {}", e);
            println!("First 200 chars of response: {}", &raw_metadata.chars().take(200).collect::<String>());
            return Err(anyhow!("Failed to parse metadata"));
        }
    };
    
    // Find MP4 files
    let mp4_files: Vec<&FileInfo> = metadata_response.files.iter()
        .filter(|file| {
            let name = file.name.to_lowercase();
            name.ends_with(".mp4") && !name.contains("_text_")
        })
        .collect();
        
    if mp4_files.is_empty() {
        return Err(anyhow!("No MP4 files found for {}", identifier));
    }
    
    // Use the largest MP4 file
    let mp4_file = mp4_files.iter()
        .max_by_key(|file| file.size.unwrap_or(0u64))
        .expect("Should have an MP4 file");
        
    // Construct download URL
    let download_url = format!("https://archive.org/download/{}/{}", identifier, mp4_file.name);
    
    // Get metadata fields for filename
    let meta = &metadata_response.metadata;
    let title = meta.title.as_deref().unwrap_or(identifier);
    let year = meta.year.as_deref().unwrap_or("");
    
    // Clean the title for use in filename
    let clean_title = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c
        })
        .collect::<String>()
        .trim()
        .to_string();
        
    // Create filename with metadata - keep it shorter
    let filename = if year.is_empty() {
        format!("{}.ia.mp4", clean_title)
    } else {
        format!("{}.{}.ia.mp4", clean_title, year)
    };
    
    // Full path to save the file
    let filepath = format!("{}/{}", output_dir, filename);
    
    // Create companion metadata JSON for TV Guide
    let json_filename = filepath.replace(".mp4", ".json");
    
    // Extract and structure the TV Guide metadata
    let tv_metadata = extract_tv_guide_metadata(&metadata_response, identifier)?;
    
    // Write the metadata to a JSON file
    let json_content = serde_json::to_string_pretty(&tv_metadata)?;
    std::fs::write(&json_filename, json_content)?;
    
    // Download the file
    let response = client
        .get(&download_url)
        .send()
        .await
        .context("Failed to start download")?;
        
    let total_size = response
        .content_length()
        .unwrap_or(0);
        
    // Create a progress bar for the download with fixed-width
    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")?  // Using wide_bar instead of bar
        .progress_chars("█▓▒░-"));
    
    // Create file to download to
    let mut file = File::create(&filepath)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    
    // Process the stream of bytes
    while let Some(item) = stream.next().await {
        let chunk = item.context("Error while downloading file")?;
        file.write_all(&chunk)
            .context("Error while writing to file")?;
            
        let new = downloaded + (chunk.len() as u64);
        downloaded = new;
        pb.set_position(new);
    }
    
    // Finish the progress bar
    pb.finish_with_message(format!("Downloaded {}", &filename));
    
    Ok(())
}

// Helper function to format file sizes in human-readable format
fn format_size(size_bytes: usize) -> String {
    if size_bytes < 1024 {
        return format!("{} B", size_bytes);
    }
    
    let kb = size_bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{:.1} KB", kb);
    }
    
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{:.1} MB", mb);
    }
    
    let gb = mb / 1024.0;    
    return format!("{:.2} GB", gb);
}

// Custom deserializer for file size (handles string and numeric values)
fn deserialize_size<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct SizeVisitor;

    impl<'de> serde::de::Visitor<'de> for SizeVisitor {
        type Value = Option<u64>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or integer size")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            // Try to parse string as a number
            match value.parse::<u64>() {
                Ok(n) => Ok(Some(n)),
                Err(_) => Ok(None),
            }
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
    }
    
    // Find an appropriate thumbnail
    let thumbnail_url = format!("https://archive.org/services/img/{}", identifier);
    
    // Extract duration if possible
    let duration = find_video_duration(response);
    
    // Categorize content based on title and description
    let category = categorize_content(&title, &description);
    
    // Extract tags for better searching
    let mut tags = Vec::new();
    if let Some(subject) = &meta.subject {
        tags = subject.split(",")
            .map(|s| s.trim().to_string())
            .collect();
    }
    
    // Determine channel number and station callsign based on content category
    let (channel_number, station_callsign) = assign_channel_and_callsign(&category, &station, &tags);
    
    // Generate realistic TV Guide timeslots
    let (start_time, end_time) = calculate_program_times(&duration, identifier);
    
    // Determine day of week (rotated to distribute content throughout the week)
    let day_of_week = determine_day_of_week(identifier);
    
    // Featured status - either special content or longer format programs
    let duration_mins = duration.split_whitespace()
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30);
    let is_featured = duration_mins > 60 || 
                      tags.iter().any(|tag| tag.to_lowercase().contains("special"));
    
    // Current download date
    let now = SystemTime::now();
    let download_date = format!("{}", now.duration_since(UNIX_EPOCH).unwrap().as_secs());
    
    Ok(TvGuideMetadata {
        title,
        station,
        description,
        year,
        duration,
        category,
        channel_number,
        timeslot: format!("{} - {}", start_time, end_time),
        day_of_week,
        start_time,
        end_time,
        thumbnail_url,
        tags,
        original_id: identifier.to_string(),
        download_date,
        station_callsign,
        is_featured,
    })
}

// Assign a TV channel number and callsign based on content category
fn assign_channel_and_callsign(category: &str, creator: &str, tags: &[String]) -> (u8, String) {
    let category_lower = category.to_lowercase();
    let creator_lower = creator.to_lowercase();
    
    // Channel assignment based on content category
    if category_lower.contains("news") || creator_lower.contains("news") {
        // News channels
        if creator_lower.contains("cbs") { return (19, "WCIO".to_string()); }
        if creator_lower.contains("abc") { return (5, "WEWS".to_string()); }
        if creator_lower.contains("nbc") { return (3, "WKYC".to_string()); }
        if creator_lower.contains("fox") { return (8, "WJW".to_string()); }
        return (5, "WEWS".to_string()); // Default news channel
    } 
    else if category_lower.contains("movie") || category_lower.contains("film") || 
             tags.iter().any(|t| t.to_lowercase().contains("movie")) {
        return (4, "WUAB".to_string()); // Movie channel
    }
    else if category_lower.contains("documentary") || 
             creator_lower.contains("pbs") || 
             creator_lower.contains("discovery") {
        return (25, "WVIZ".to_string()); // Documentary/PBS channel
    }
    else if category_lower.contains("comedy") || 
             category_lower.contains("sitcom") ||
             tags.iter().any(|t| t.to_lowercase().contains("comedy")) {
        return (8, "WJW".to_string()); // Comedy channel
    }
    else if category_lower.contains("drama") ||
             category_lower.contains("series") {
        return (3, "WKYC".to_string()); // Drama channel
    }
    else if category_lower.contains("kids") || 
             category_lower.contains("animation") || 
             category_lower.contains("children") {
        return (43, "WUAB".to_string()); // Kids channel
    }
    else if category_lower.contains("sport") {
        return (35, "ESPN".to_string()); // Sports channel
    }
    
    // For unknown categories, assign a channel based on the hash of the creator name
    let hash_value = creator.bytes().fold(0u8, |acc, b| acc.wrapping_add(b));
    let channel = (hash_value % 40) + 2; // Channels 2-42
    
    // Generate a random callsign for unknown channels
    let callsign = format!("W{}{}{}", 
        (b'A' + (hash_value % 26)) as char,
        (b'A' + ((hash_value / 2) % 26)) as char,
        (b'A' + ((hash_value / 3) % 26)) as char);
    
    (channel, callsign)
}

// Calculate realistic program start and end times based on duration
fn calculate_program_times(duration: &str, item_id: &str) -> (String, String) {
    // Extract minutes from duration string (e.g. "120 min" -> 120)
    let minutes = duration.split_whitespace()
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30); // Default to 30 minutes
    
    // Map to standard TV blocks (30 min, 60 min, 90 min, 120 min)
    let block_size = if minutes <= 30 { 30 }
        else if minutes <= 60 { 60 }
        else if minutes <= 90 { 90 }
        else { 120 };
    
    // Standard TV timeslots
    let timeslots = [
        "6:00 PM", "6:30 PM", "7:00 PM", "7:30 PM", "8:00 PM", 
        "8:30 PM", "9:00 PM", "9:30 PM", "10:00 PM", "10:30 PM"
    ];
    
    // Deterministically select a starting timeslot but leave room for the program duration
    let max_index = timeslots.len() - (block_size / 30);
    let hash_value = item_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let start_index = (hash_value as usize) % max_index;
    
    let end_index = start_index + (block_size / 30);
    let end_index = if end_index >= timeslots.len() { timeslots.len() - 1 } else { end_index };
    
    (timeslots[start_index].to_string(), timeslots[end_index].to_string())
}

// Determine day of week to distribute content across the week
fn determine_day_of_week(item_id: &str) -> String {
    let days = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    let hash = item_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let index = hash as usize % days.len();
    days[index].to_string()
}

// Structure for TV Guide metadata
#[derive(Serialize, Deserialize, Debug)]
struct TvGuideMetadata {
    title: String,
    station: String,
    description: String,
    year: String,
    duration: String,
    category: String,
    channel_number: u8,
    timeslot: String,
    day_of_week: String,
    start_time: String,
    end_time: String,
    thumbnail_url: String,
    tags: Vec<String>,
    original_id: String,
    download_date: String,
    station_callsign: String,
    is_featured: bool,
}

// Helper to find video duration from various file metadata fields
fn find_video_duration(response: &MetadataResponse) -> String {
    for file in &response.files {
        if file.name.ends_with(".mp4") {
            // Try various fields that might contain duration
            if let Some(runtime) = &file.runtime {
                return runtime.clone();
            }
            if let Some(length) = &file.length {
                return length.clone();
            }
        }
    }
    // Default value
    "00:30:00".to_string()
}

// Categorize content based on title and description
fn categorize_content(title: &str, description: &str) -> String {
    let combined = format!("{} {}", title, description).to_lowercase();
    
    // Check for common program types
    if combined.contains("news") || combined.contains("report") || combined.contains("update") {
        return "News".to_string();
    } else if combined.contains("sport") || combined.contains("game") || 
              combined.contains("match") || combined.contains("championship") {
        return "Sports".to_string();
    } else if combined.contains("commercial") || combined.contains("ad") || combined.contains("advertisement") {
        return "Commercial".to_string();
    } else if combined.contains("cartoon") || combined.contains("animation") {
        return "Cartoon".to_string();
    } else if combined.contains("documentary") || combined.contains("educational") {
        return "Documentary".to_string();
    } else if combined.contains("movie") || combined.contains("film") {
        return "Movie".to_string();
    } else if combined.contains("show") || combined.contains("series") || combined.contains("episode") {
        return "TV Show".to_string();
    } else {
        return "Entertainment".to_string(); // Default category
    }
}



// Function to manually extract identifiers and titles from response JSON
fn try_extract_identifiers(json_text: &str) -> Result<Vec<Document>> {
    let mut docs = Vec::new();
    
    // Very simple JSON extraction to be more robust
    let v: serde_json::Value = serde_json::from_str(json_text)?;
    
    // Try different paths to find the docs array
    let doc_array = match &v["response"]["docs"] {
        serde_json::Value::Array(arr) => Some(arr),
        _ => None,
    };
    
    if let Some(array) = doc_array {
        for (i, item) in array.iter().enumerate() {
            if i >= 20 { // Limit to 20 items for safety
                break;
            }
            
            // Extract basic info
            let identifier = item["identifier"].as_str().unwrap_or("unknown").to_string();
            let title = item["title"].as_str().unwrap_or("(No Title)").to_string();
            
            // Only add items with valid identifiers
            if identifier != "unknown" {
                // Fetch file size - try different possible fields
                let item_size = item["size"].as_u64()
                    .or_else(|| item["item_size"].as_u64()) // Try alternate field name
                    .or_else(|| { // Try to parse from string size field
                        item["size"].as_str()
                            .and_then(|s| s.parse::<u64>().ok())
                            .or_else(|| item["item_size"].as_str().and_then(|s| s.parse::<u64>().ok()))
                    })
                    .map(|s| s as usize);
                
                let doc = Document {
                    identifier,
                    title: Some(title),
                    description: None,
                    mediatype: Some("movies".to_string()),
                    year: item["year"].as_str().map(|s| s.to_string())
                        .or_else(|| item["year"].as_i64().map(|i| i.to_string())),
                    creator: extract_string_array(item, "creator"),
                    subject: extract_string_array(item, "subject"),
                    item_size: item_size,
                    downloads: item["downloads"].as_u64().map(|d| d as usize),
                };
                
                docs.push(doc);
            }
        }
    }
    
    // If we don't have file sizes yet, we can estimate them from average bitrates
    // This helps provide useful size information in the UI
    for doc in &mut docs {
        if doc.item_size.is_none() {
            // Attempt to estimate size based on typical bitrates for video content
            // Standard definition video: ~2 Mbps
            let estimated_size = 15 * 1024 * 1024; // Default to ~15MB for a short clip
            doc.item_size = Some(estimated_size);
        }
    }
    
    Ok(docs)
}

// Helper to extract string arrays from JSON
fn extract_string_array(item: &serde_json::Value, key: &str) -> Vec<String> {
    match &item[key] {
        serde_json::Value::Array(arr) => {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        },
        serde_json::Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

// Start the existing video server using npm scripts
async fn start_server() -> Result<()> {
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush()?;
    
    println!("🖥️  Starting Channel Surfer Server...");
    println!("===============================");
    
    // Path to the project root
    let project_dir = std::env::current_dir()?;
    
    // Check if package.json exists
    let package_json = project_dir.join("package.json");
    if !package_json.exists() {
        println!("⚠️  Could not find package.json at {}", package_json.display());
        println!("Please start your server manually from a terminal.");
        prompt_user("\nPress Enter to return to the main menu...")?;
        return Ok(());
    }
    
    // Check if npm is installed
    let npm_check = std::process::Command::new("npm")
        .arg("--version")
        .output();
    
    if let Err(_) = npm_check {
        println!("⚠️  npm not found. Please install Node.js and npm to run the server.");
        prompt_user("\nPress Enter to return to the main menu...")?;
        return Ok(());
    }
    
    println!("Starting your Vite development server...");
    
    // Start the development server with npm run dev
    let server_process = std::process::Command::new("npm")
        .arg("run")
        .arg("dev")
        .current_dir(&project_dir)
        .spawn()?;
    
    println!("\n🎬 Server started successfully with process ID: {}", server_process.id());
    println!("\n🌐 Access Channel Surfer at: http://localhost:5173");  // Vite uses port 5173 by default
    println!("\n⚠️  Note: Server will continue running in the background.");
    println!("   You can stop it by pressing Ctrl+C in its terminal or");
    println!("   by ending process ID {} when you're done.", server_process.id());
    
    prompt_user("\nPress Enter to return to the main menu...")?;
    
    Ok(())
}



fn prompt_user(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    Ok(input.trim().to_string())
}
