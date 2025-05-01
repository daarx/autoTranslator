use std::fmt::{Display};
use std::fs::File;

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::multipart;
use std::io::{Read, Write};
use std::vec;
use opencv::core::Rect;
use soloud::{AudioExt, LoadExt, audio, Soloud};
use tokio;
use crate::TextToSpeechLanguage::{English, Finnish, Japanese};
use opencv::prelude::*;
use opencv::videoio::VideoCapture;

enum TextToSpeechLanguage {
    Japanese,
    English,
    Finnish
}

impl Display for TextToSpeechLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Japanese => f.write_str("ja"),
            English => f.write_str("en"),
            Finnish => f.write_str("fi"),
        }
    }
}

struct TranslationResponse {
    en_translation: String,
    fi_translation: String,
}

struct UsageOptions {
    playback_en: bool,
    playback_fi: bool,
    use_translation: bool,
    half_screen: bool,
    debug_printing: bool
}

struct CameraCapture {
    cap: VideoCapture,
    height: i32,
    width: i32
}

impl CameraCapture {
    fn new(width: i32, height: i32) -> Self {
        let mut camera_capture = CameraCapture {
            cap: VideoCapture::new(0, opencv::videoio::CAP_DSHOW).unwrap(),
            height,
            width
        };

        if !camera_capture.cap.is_opened().unwrap() {
            panic!("Camera didn't open properly!");
        }
        
        camera_capture.cap.set(opencv::videoio::CAP_PROP_FRAME_WIDTH, width as f64).unwrap();
        camera_capture.cap.set(opencv::videoio::CAP_PROP_FRAME_HEIGHT, height as f64).unwrap();
        
        camera_capture.height = camera_capture.cap.get(opencv::videoio::CAP_PROP_FRAME_HEIGHT).unwrap() as i32;
        camera_capture.width = camera_capture.cap.get(opencv::videoio::CAP_PROP_FRAME_WIDTH).unwrap() as i32;

        camera_capture
    }

    fn capture_image(&mut self, half_screen: bool) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut mat = Mat::default();

        if !self.cap.read(&mut mat).unwrap() {
            panic!("Could not capture image!");
        }

        if half_screen {
            let crop_rect = Rect::new(0, self.height / 2, self.width, self.height / 2);
            let cropped_mat = mat.roi(crop_rect).unwrap();

            opencv::imgcodecs::imwrite_def("output_image.jpg", &cropped_mat).unwrap();
        } else {
            opencv::imgcodecs::imwrite_def("output_image.jpg", &mat).unwrap();
        }
        
        let mut file = File::open("output_image.jpg")?;
        let mut bytes_vector = Vec::new();
        file.read_to_end(&mut bytes_vector)?;
        Ok(bytes_vector)
    }
}

struct OcrClient {
    client: reqwest::Client,
    headers: HeaderMap,
}

impl OcrClient {
    fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Ocp-Apim-Subscription-Key",
            HeaderValue::from_str(azure_ocr_key().as_str()).unwrap(),
        );

        Self {
            client: reqwest::Client::new(),
            headers
        }
    }

    async fn make_request(&self, buffer: Vec<u8>, usage_options: &UsageOptions) -> Result<String, Box<dyn std::error::Error>> {
        let part = multipart::Part::bytes(buffer).mime_str("image/jpg")?;
        let form = multipart::Form::new().part("file", part);

        let response = self.client
            .post(azure_ocr_url())
            .headers(self.headers.clone())
            .multipart(form)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        if usage_options.debug_printing {
            println!("{}", response.to_string());
        }

        let mut output = String::with_capacity(100);
        if let Some(regions) = response["regions"].as_array() {
            for region in regions {
                for line in region["lines"].as_array().unwrap_or(&vec![]) {
                    let words: Vec<String> = line["words"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|w| w["text"].as_str().unwrap_or("").trim().to_string())
                        .collect();
                    output.push_str(&words.join(""));
                }
            }
        } else {
            println!("No text detected.");
        }

        // output.push_str("今日は俺の名前はヘンリクだよ。よろしくお願いします。"); // Can be used for debug purposes.

        Ok(output)
    }
}

struct TextToSpeechClient {
    client: reqwest::Client,
    headers: HeaderMap,
}

impl TextToSpeechClient {
    fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Ocp-Apim-Subscription-Key",
            HeaderValue::from_str(azure_text_to_speech_key().as_str()).unwrap(),
        );
        headers.insert(
            "Content-Type",
            HeaderValue::from_static("application/ssml+xml"),
        );
        headers.insert(
            "X-Microsoft-OutputFormat",
            HeaderValue::from_static("audio-16khz-128kbitrate-mono-mp3"),
        );
        headers.insert("User-Agent", HeaderValue::from_static("Reqwest"));

        Self {
            client: reqwest::Client::new(),
            headers
        }
    }

    async fn make_request(&self, text: &String, language: TextToSpeechLanguage) -> Result<(), Box<dyn std::error::Error>> {
        let body = match language {
            Japanese => format!("<speak version='1.0' xml:lang='ja-JP'><voice xml:lang='ja-JP' xml:gender='Female' name='ja-JP-NanamiNeural'>{}</voice></speak>", text),
            English => format!("<speak version='1.0' xml:lang='en-US'><voice xml:lang='en-US' xml:gender='Female' name='en-US-AvaMultilingualNeural'>{}</voice></speak>", text),
            Finnish => format!("<speak version='1.0' xml:lang='fi-FI'><voice xml:lang='fi-FI' xml:gender='Female' name='fi-FI-SelmaNeural'>{}</voice></speak>", text)
        };

        let response = self.client
            .post(azure_text_to_speech_url())
            .headers(self.headers.clone())
            .body(body)
            .send()
            .await?;

        let response_bytes = response.bytes().await?.to_vec();

        // Save the audio to a file
        let mut file = File::create("output_audio.mp3").expect("Failed to create audio file");
        let _ = file
            .write_all(response_bytes.as_slice())
            .expect("Failed to write to file");

        Ok(())
    }
}

struct TranslatorClient {
    client: reqwest::Client,
    headers: HeaderMap,
}

impl TranslatorClient {
    fn new() -> Self {
        let azure_translator_key = azure_translator_key();
        let azure_region = azure_region();

        let mut headers = HeaderMap::new();
        headers.insert("Ocp-Apim-Subscription-Key", HeaderValue::from_str(azure_translator_key.as_str()).unwrap());
        headers.insert("Ocp-Apim-Subscription-Region", HeaderValue::from_str(azure_region.as_str()).unwrap());
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("User-Agent", HeaderValue::from_static("Reqwest"));

        Self {
            client: reqwest::Client::new(),
            headers
        }
    }
    async fn make_request(&self, text: &String, output_languages: &[TextToSpeechLanguage]) -> Result<(TranslationResponse), Box<dyn std::error::Error>> {
        if output_languages.is_empty() {
            return Ok(TranslationResponse { en_translation: String::new(), fi_translation: String::new() });
        }

        let azure_translator_url = azure_translator_url();
        let body = format!("[{{ \"Text\": \"{}\" }}]", text);
        let output_language = output_languages.iter().map(|lang| lang.to_string()).collect::<Vec<String>>().join(",");

        let response = self.client
            .post(format!("{}&to={}", azure_translator_url, output_language))
            .headers(self.headers.clone())
            .body(body)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let mut translationResponse = TranslationResponse {
            en_translation: String::new(),
            fi_translation: String::new(),
        };

        match response[0]["translations"].as_array() {
            Some(translations) => {
                translations.iter().for_each(|translation| {
                    if translation["to"].as_str().unwrap_or("en") == "fi" {
                        translationResponse.fi_translation = translation["text"].to_string();
                    } else if translation["to"].as_str().unwrap_or("en") == "en" {
                        translationResponse.en_translation = translation["text"].to_string();
                    }
                });
            },
            None => {
                println!("Did not get translations");
            }
        }

        Ok(translationResponse)
    }
}

struct AudioPlayer {
    player: Soloud
}

impl AudioPlayer {
    fn new() -> Self {
        Self {
            player: Soloud::default().unwrap()
        }
    }

    async fn play_audio(&self, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut wav = audio::Wav::default();
        let mut file = File::open(filename)?;
        let mut file_vector = Vec::new();
        file.read_to_end(&mut file_vector)?;
        wav.load_mem(file_vector.as_slice())?;
        self.player.play(&wav);
        while self.player.voice_count() > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok(); // Load settings from .env file into environment variables

    let mut camera = CameraCapture::new(3840, 2160);
    let ocr_client = OcrClient::new();
    let text_to_speech_client = TextToSpeechClient::new();
    let translator_client = TranslatorClient::new();
    let audio_player = AudioPlayer::new();

    use text_io::read;

    println!("Press enter to capture, q-enter to quit, [fethd]-enter to toggle mode:");
    let mut line: String = read!("{}\n");

    let mut usage_options = UsageOptions { playback_en: false, playback_fi: false, use_translation: false, half_screen: false, debug_printing: false };

    while !line.contains("q") {
        if line.contains("f") { usage_options.playback_fi = !usage_options.playback_fi };
        if line.contains("e") { usage_options.playback_en = !usage_options.playback_en };
        if line.contains("t") { usage_options.use_translation = !usage_options.use_translation };
        if line.contains("h") { usage_options.half_screen = !usage_options.half_screen };
        if line.contains("d") { usage_options.debug_printing = !usage_options.debug_printing };

        match capture_process_playback(&mut camera, &ocr_client, &text_to_speech_client, &translator_client, &audio_player, &usage_options).await {
            Ok(_) => (),
            Err(e) => eprintln!("{}", e),
        }

        println!("Press enter to capture, q-enter to quit, [fet]-enter to toggle mode:");
        line = read!("{}\n");
    }
}

async fn capture_process_playback(camera: &mut CameraCapture, ocr_client: &OcrClient, text_to_speech_client: &TextToSpeechClient, translator_client: &TranslatorClient, audio_player: &AudioPlayer, usage_options: &UsageOptions) -> Result<(), Box<dyn std::error::Error>> {
    let image_buffer = if use_test_file().parse()? {
        load_image_from_disk()?
    } else {
        camera.capture_image(usage_options.half_screen)?
    };
    
    let extracted_text = ocr_client.make_request(image_buffer, &usage_options).await?;

    println!("Extracted text JP: {}", &extracted_text);

    let mut languages = Vec::new();
    if !usage_options.use_translation { languages.clear(); };
    if usage_options.use_translation { languages.push(English); languages.push(Finnish); };

    let translated_text_future = translator_client.make_request(&extracted_text, languages.as_slice());

    text_to_speech_client.make_request(&extracted_text, Japanese).await?;
    audio_player.play_audio("output_audio.mp3").await?;

    let translated_text = translated_text_future.await?;

    if !translated_text.en_translation.is_empty() {
        println!("Translated text EN: {}", &translated_text.en_translation);
    }

    if !translated_text.en_translation.is_empty() && usage_options.playback_en {
        text_to_speech_client.make_request(&translated_text.en_translation, English).await?;
        audio_player.play_audio("output_audio.mp3").await?;
    }

    if !translated_text.fi_translation.is_empty() {
        println!("Translated text FI: {}", &translated_text.fi_translation);
    }

    if !translated_text.fi_translation.is_empty() && usage_options.playback_fi {
        text_to_speech_client.make_request(&translated_text.fi_translation, Finnish).await?;
        audio_player.play_audio("output_audio.mp3").await?;
    }

    Ok(())
}

fn load_image_from_disk() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut file = File::open("test_image.jpg")?;
    let mut bytes_vector = Vec::new();
    file.read_to_end(&mut bytes_vector)?;

    Ok(bytes_vector)
}

fn azure_ocr_url() -> String { dotenv::var("AZURE_OCR_URL").expect("Couldn't find environment variable AZURE_OCR_URL") }
fn azure_text_to_speech_url() -> String { dotenv::var("AZURE_TEXT_TO_SPEECH_URL").expect("Couldn't find environment variable AZURE_TEXT_TO_SPEECH_URL") }
fn azure_ocr_key() -> String { dotenv::var("AZURE_OCR_KEY").expect("Couldn't find environment variable AZURE_OCR_KEY") }
fn azure_text_to_speech_key() -> String { dotenv::var("AZURE_TEXT_TO_SPEECH_KEY").expect("Couldn't find environment variable AZURE_TEXT_TO_SPEECH_KEY") }
fn use_aws_text_to_speech() -> String { dotenv::var("USE_AWS_TEXT_TO_SPEECH").expect("Couldn't find environment variable USE_AWS_TEXT_TO_SPEECH") }
fn azure_translator_url() -> String { dotenv::var("AZURE_TRANSLATOR_URL").expect("Couldn't find environment variable AZURE_TRANSLATOR_URL") }
fn azure_translator_key() -> String { dotenv::var("AZURE_TRANSLATOR_KEY").expect("Couldn't find environment variable AZURE_TRANSLATOR_KEY") }
fn azure_region() -> String { dotenv::var("AZURE_REGION").expect("Couldn't find environment variable AZURE_REGION") }
fn camera_sharpness() -> String { dotenv::var("CAMERA_SHARPNESS").expect("Couldn't find environment variable CAMERA_SHARPNESS") }
fn camera_zoom() -> String { dotenv::var("CAMERA_ZOOM").expect("Couldn't find environment variable CAMERA_ZOOM") }
fn camera_brightness() -> String { dotenv::var("CAMERA_BRIGHTNESS").expect("Couldn't find environment variable CAMERA_BRIGHTNESS") }
fn camera_contrast() -> String { dotenv::var("CAMERA_CONTRAST").expect("Couldn't find environment variable CAMERA_CONTRAST") }
fn camera_saturation() -> String { dotenv::var("CAMERA_SATURATION").expect("Couldn't find environment variable CAMERA_SATURATION") }
fn use_test_file() -> String { dotenv::var("USE_TEST_FILE").expect("Couldn't find environment variable USE_TEST_FILE") }