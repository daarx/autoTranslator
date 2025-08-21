mod audio_player;
mod azure_clients;
mod camera_capture;
mod google_client;
mod utils;

use std::fs::File;

use crate::audio_player::audio_player::AudioPlayer;
use crate::azure_clients::azure_clients::{
    AzureOcrClient, AzureTextToSpeechClient, AzureTranslatorClient,
};
use crate::camera_capture::camera_capture::CameraCapture;
use crate::google_client::google_client::GoogleCloudClient;
use crate::utils::utils::TextToSpeechLanguage::{English, Finnish, Japanese, Swedish};
use crate::utils::utils::UsageOptions;
use std::io::{Read};
use tokio;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok(); // Load settings from .env file into environment variables

    let mut camera = CameraCapture::new(3840, 2160);
    let azure_ocr_client = AzureOcrClient::new();
    let google_cloud_client = GoogleCloudClient::new();
    let azure_text_to_speech_client = AzureTextToSpeechClient::new();
    let azure_translator_client = AzureTranslatorClient::new();
    let audio_player = AudioPlayer::new();

    use text_io::read;

    println!("Press enter to capture, q-enter to quit, [fethdc]-enter to toggle mode:");
    let mut line: String = read!("{}\n");

    let mut usage_options = UsageOptions {
        playback_en: false,
        playback_fi: false,
        use_translation: true,
        half_screen: false,
        debug_printing: false,
        color_correction: false,
    };

    while !line.contains("q") {
        if line.contains("f") {
            usage_options.playback_fi = !usage_options.playback_fi
        };
        if line.contains("e") {
            usage_options.playback_en = !usage_options.playback_en
        };
        if line.contains("t") {
            usage_options.use_translation = !usage_options.use_translation
        };
        if line.contains("h") {
            usage_options.half_screen = !usage_options.half_screen
        };
        if line.contains("d") {
            usage_options.debug_printing = !usage_options.debug_printing
        };
        if line.contains("c") {
            usage_options.color_correction = !usage_options.color_correction
        };

        match capture_process_playback(
            &mut camera,
            &azure_ocr_client,
            &azure_text_to_speech_client,
            &azure_translator_client,
            &google_cloud_client,
            &audio_player,
            &usage_options,
        )
        .await
        {
            Ok(_) => (),
            Err(e) => eprintln!("{}", e),
        }

        println!("Press enter to capture, q-enter to quit, [fethdc]-enter to toggle mode:");
        line = read!("{}\n");
    }
}

async fn capture_process_playback(
    camera: &mut CameraCapture,
    azure_ocr_client: &AzureOcrClient,
    azure_text_to_speech_client: &AzureTextToSpeechClient,
    azure_translator_client: &AzureTranslatorClient,
    google_cloud_client: &GoogleCloudClient,
    audio_player: &AudioPlayer,
    usage_options: &UsageOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let image_buffer = if use_test_file().parse()? {
        load_image_from_disk()?
    } else {
        camera.capture_image(usage_options.half_screen, usage_options.color_correction)?
    };

    let extracted_text = google_cloud_client
        .make_ocr_request(image_buffer, &usage_options)
        .await?;

    println!("{}\n", &extracted_text);

    let mut languages = Vec::new();
    if !usage_options.use_translation {
        languages.clear();
    };
    if usage_options.use_translation {
        languages.push(English);
        languages.push(Finnish);
        languages.push(Swedish);
    };

    let translated_text_future =
        google_cloud_client.make_trans_request(&extracted_text, languages.as_slice());

    google_cloud_client
        .make_tts_request(&extracted_text, Japanese)
        .await?;

    audio_player.play_audio("output_audio.mp3").await?;

    let translated_text = translated_text_future.await?;

    if !translated_text.en_translation.is_empty() {
        println!("{}\n", &translated_text.en_translation);
    }

    if !translated_text.en_translation.is_empty() && usage_options.playback_en {
        azure_text_to_speech_client
            .make_request(&translated_text.en_translation, English)
            .await?;
        audio_player.play_audio("output_audio.mp3").await?;
    }

    if !translated_text.fi_translation.is_empty() {
        println!("{}\n", &translated_text.fi_translation);
    }

    if !translated_text.fi_translation.is_empty() && usage_options.playback_fi {
        azure_text_to_speech_client
            .make_request(&translated_text.fi_translation, Finnish)
            .await?;
        audio_player.play_audio("output_audio.mp3").await?;
    }

    if !translated_text.sv_translation.is_empty() {
        println!("{}\n", &translated_text.sv_translation);
    }

    Ok(())
}

fn load_image_from_disk() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut file = File::open("test_image.jpg")?;
    let mut bytes_vector = Vec::new();
    file.read_to_end(&mut bytes_vector)?;

    Ok(bytes_vector)
}

fn azure_ocr_url() -> String {
    dotenv::var("AZURE_OCR_URL").expect("Couldn't find environment variable AZURE_OCR_URL")
}
fn azure_text_to_speech_url() -> String {
    dotenv::var("AZURE_TEXT_TO_SPEECH_URL")
        .expect("Couldn't find environment variable AZURE_TEXT_TO_SPEECH_URL")
}
fn azure_ocr_key() -> String {
    dotenv::var("AZURE_OCR_KEY").expect("Couldn't find environment variable AZURE_OCR_KEY")
}
fn azure_text_to_speech_key() -> String {
    dotenv::var("AZURE_TEXT_TO_SPEECH_KEY")
        .expect("Couldn't find environment variable AZURE_TEXT_TO_SPEECH_KEY")
}
fn azure_translator_url() -> String {
    dotenv::var("AZURE_TRANSLATOR_URL")
        .expect("Couldn't find environment variable AZURE_TRANSLATOR_URL")
}
fn azure_translator_key() -> String {
    dotenv::var("AZURE_TRANSLATOR_KEY")
        .expect("Couldn't find environment variable AZURE_TRANSLATOR_KEY")
}
fn azure_region() -> String {
    dotenv::var("AZURE_REGION").expect("Couldn't find environment variable AZURE_REGION")
}
fn use_test_file() -> String {
    dotenv::var("USE_TEST_FILE").expect("Couldn't find environment variable USE_TEST_FILE")
}
fn threshold() -> f64 {
    dotenv::var("THRESHOLD")
        .expect("Couldn't find THRESHOLD")
        .parse()
        .unwrap()
}
