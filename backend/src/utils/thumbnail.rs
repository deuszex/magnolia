use std::path::Path;

const THUMBNAIL_MAX_WIDTH: u32 = 400;
const THUMBNAIL_MAX_HEIGHT: u32 = 400;

/// Generate a thumbnail for an image file.
/// Returns the path to the generated thumbnail, or None if generation failed.
pub async fn generate_image_thumbnail(
    source_path: &str,
    thumbnail_dir: &str,
    media_id: &str,
) -> Option<String> {
    let source = source_path.to_string();
    let thumb_dir = thumbnail_dir.to_string();
    let id = media_id.to_string();

    // Run image processing on a blocking thread
    tokio::task::spawn_blocking(move || generate_image_thumbnail_sync(&source, &thumb_dir, &id))
        .await
        .ok()
        .flatten()
}

fn generate_image_thumbnail_sync(
    source_path: &str,
    thumbnail_dir: &str,
    media_id: &str,
) -> Option<String> {
    let img = image::open(source_path).ok()?;

    let thumbnail = img.thumbnail(THUMBNAIL_MAX_WIDTH, THUMBNAIL_MAX_HEIGHT);

    // Ensure thumbnail directory exists
    std::fs::create_dir_all(thumbnail_dir).ok()?;

    let thumb_path = Path::new(thumbnail_dir).join(format!("{}.jpg", media_id));
    let thumb_path_str = thumb_path.to_string_lossy().to_string();

    thumbnail.save(&thumb_path).ok()?;

    Some(thumb_path_str)
}

/// Generate a thumbnail for a video file using ffmpeg.
/// Returns the path to the generated thumbnail, or None if ffmpeg is not available or generation failed.
pub async fn generate_video_thumbnail(
    source_path: &str,
    thumbnail_dir: &str,
    media_id: &str,
) -> Option<String> {
    // Ensure thumbnail directory exists
    tokio::fs::create_dir_all(thumbnail_dir).await.ok()?;

    let thumb_path = format!("{}/{}.jpg", thumbnail_dir, media_id);

    let output = tokio::process::Command::new("ffmpeg")
        .args([
            "-i",
            source_path,
            "-ss",
            "00:00:01", // grab frame at 1 second
            "-vframes",
            "1", // single frame
            "-vf",
            &format!(
                "scale={}:{}:force_original_aspect_ratio=decrease",
                THUMBNAIL_MAX_WIDTH, THUMBNAIL_MAX_HEIGHT
            ),
            "-y", // overwrite
            &thumb_path,
        ])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        Some(thumb_path)
    } else {
        tracing::debug!(
            "ffmpeg thumbnail generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        None
    }
}
