use std::fmt::{Display};
use std::fs::File;

use aws_config::BehaviorVersion;
use aws_sdk_polly::operation::synthesize_speech::SynthesizeSpeechOutput;
use aws_sdk_polly::types::{Engine, LanguageCode, OutputFormat, TextType, VoiceId};
use aws_sdk_polly::Client as PollyClient;
use aws_types::region::Region;
use nokhwa::pixel_format::{RgbFormat};
use nokhwa::utils::{CameraFormat, CameraIndex, ControlValueSetter, FrameFormat, KnownCameraControl, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::multipart;
use std::io::{Read, Write};
use std::vec;
use soloud::{AudioExt, LoadExt, audio, Soloud};
use tokio;
use crate::TextToSpeechLanguage::{English, Finnish, Japanese};

// Environment variables
struct EnvVars {
    azure_ocr_url: String,
    azure_text_to_speech_url: String,
    azure_ocr_key: String,
    azure_text_to_speech_key: String,
    use_aws_text_to_speech: String,
    azure_translator_url: String,
    azure_translator_key: String,
    azure_region: String,
    camera_sharpness: String,
    camera_zoom: String,
    camera_brightness: String,
    camera_contrast: String,
    camera_saturation: String,
}

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
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok(); // Load settings from .env file into environment variables

    use text_io::read;

    println!("Press enter to capture, q-enter to quit, [fet]-enter to toggle mode:");
    let mut line: String = read!("{}\n");

    let mut usage_options = UsageOptions { playback_en: false, playback_fi: false, use_translation: false };

    while !line.contains("q") {
        if line.contains("f") { usage_options.playback_fi = !usage_options.playback_fi };
        if line.contains("e") { usage_options.playback_en = !usage_options.playback_en };
        if line.contains("t") { usage_options.use_translation = !usage_options.use_translation };

        match capture_process_playback(&usage_options).await {
            Ok(_) => (),
            Err(e) => eprintln!("{}", e),
        }

        println!("Press enter to capture, q-enter to quit, [fet]-enter to toggle mode:");
        line = read!("{}\n");
    }
}

async fn capture_process_playback(usage_options: &UsageOptions) -> Result<(), Box<dyn std::error::Error>> {
    let image_buffer = if use_test_file().parse()? {
        load_image_from_disk()?
    } else {
        capture_image_from_webcam()?
    };
    let extracted_text = extract_text_from_image(image_buffer).await?;

    println!("Extracted text JP: {}", &extracted_text);

    let mut languages = Vec::new();
    if !usage_options.use_translation { languages.clear(); };
    if usage_options.use_translation { languages.push(English); languages.push(Finnish); };

    let translated_text_future = translate_text(&extracted_text, languages.as_slice());

    convert_text_to_speech(&extracted_text, Japanese).await?;
    playback_sound().await?;

    let translated_text = translated_text_future.await?;

    if !translated_text.en_translation.is_empty() {
        println!("Translated text EN: {}", &translated_text.en_translation);
    }

    if !translated_text.en_translation.is_empty() && usage_options.playback_en {
        convert_text_to_speech(&translated_text.en_translation, English).await?;
        playback_sound().await?;
    }

    if !translated_text.fi_translation.is_empty() {
        println!("Translated text FI: {}", &translated_text.fi_translation);
    }

    if !translated_text.fi_translation.is_empty() && usage_options.playback_fi {
        convert_text_to_speech(&translated_text.fi_translation, Finnish).await?;
        playback_sound().await?;
    }

    Ok(())
}

fn capture_image_from_webcam() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // first camera in system
    let index = CameraIndex::Index(0);
    // request the absolute highest resolution CameraFormat that can be decoded to RGB.
    // let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::HighestResolution(Resolution::new(1280, 720)));
    // let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::NV12, 1)));
    // let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(CameraFormat::new(Resolution::new(1280, 720), FrameFormat::NV12, 1)));
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
    // make the camera
    let mut camera = Camera::new(index, requested)?;

    // camera.set_camera_control(KnownCameraControl::Gain, ControlValueSetter::Integer(0))?; // Might not be changeable
    // camera.set_camera_control(KnownCameraControl::Exposure, ControlValueSetter::Integer(-6))?; // Might not be changeable
    // camera.set_camera_control(KnownCameraControl::Focus, ControlValueSetter::Integer(68))?; // Might not be changeable
    camera.set_camera_control(KnownCameraControl::Sharpness, ControlValueSetter::Integer(camera_sharpness().parse()?))?; // (0-255, default: 72)
    camera.set_camera_control(KnownCameraControl::Zoom, ControlValueSetter::Integer(camera_zoom().parse()?))?; // (1-5, default: 1)
    camera.set_camera_control(KnownCameraControl::Brightness, ControlValueSetter::Integer(camera_brightness().parse()?))?; // (0-255, default: 128)
    camera.set_camera_control(KnownCameraControl::Contrast, ControlValueSetter::Integer(camera_contrast().parse()?))?; // (0-255, default: 32)
    camera.set_camera_control(KnownCameraControl::Saturation, ControlValueSetter::Integer(camera_saturation().parse()?))?; // (0-255, default: 32)

    // camera.camera_controls_known_camera_controls()?.iter().for_each(|control| { println!("Known control: {:?}", control); });
    // camera.camera_controls()?.iter().for_each(|control| { println!("Control: {:?}", control); });
    // camera.compatible_camera_formats()?.iter().for_each(|format| { println!("Format: {:?}", format); });

    camera.open_stream()?;

    // get a frame
    let frame = camera.frame()?;
    // decode into an ImageBuffer
    let decoded = frame.decode_image::<RgbFormat>()?;

    decoded.save("output_image.jpg")?;

    let mut output_file = File::open("output_image.jpg")?;
    let mut output_vec = Vec::new();
    output_file.read_to_end(&mut output_vec)?;

    // exit(0);

    Ok(output_vec)
}

fn load_image_from_disk() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut file = File::open("test_image.jpg")?;
    let mut bytes_vector = Vec::new();
    file.read_to_end(&mut bytes_vector)?;

    Ok(bytes_vector)
}

async fn extract_text_from_image(buffer: Vec<u8>) -> Result<String, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Ocp-Apim-Subscription-Key",
        HeaderValue::from_str(azure_ocr_key().as_str())?,
    );

    let part = multipart::Part::bytes(buffer).mime_str("image/jpg")?;
    let form = multipart::Form::new().part("file", part);

    let client = reqwest::Client::new();
    let response = client
        .post(azure_ocr_url())
        .headers(headers)
        .multipart(form)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let mut output = "".to_string();
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

async fn convert_text_to_speech(extracted_text: &str, language: TextToSpeechLanguage) -> Result<(), Box<dyn std::error::Error>> {
    if use_aws_text_to_speech().trim().parse()? {
        convert_text_to_speech_with_aws(extracted_text, language).await
    } else {
        convert_text_to_speech_with_azure(extracted_text, language).await
    }
}

async fn convert_text_to_speech_with_azure(text: &str, language: TextToSpeechLanguage) -> Result<(), Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Ocp-Apim-Subscription-Key",
        HeaderValue::from_str(azure_text_to_speech_key().as_str())?,
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

    let body = match language {
        TextToSpeechLanguage::Japanese => format!("<speak version='1.0' xml:lang='ja-JP'><voice xml:lang='ja-JP' xml:gender='Female' name='ja-JP-NanamiNeural'>{}</voice></speak>", text),
        English => format!("<speak version='1.0' xml:lang='en-US'><voice xml:lang='en-US' xml:gender='Female' name='en-US-AvaMultilingualNeural'>{}</voice></speak>", text),
        Finnish => format!("<speak version='1.0' xml:lang='fi-FI'><voice xml:lang='fi-FI' xml:gender='Female' name='fi-FI-SelmaNeural'>{}</voice></speak>", text)
    };

    let client = reqwest::Client::new();
    let response = client
        .post(azure_text_to_speech_url())
        .headers(headers)
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

async fn translate_text(text: &str, output_languages: &[TextToSpeechLanguage]) -> Result<TranslationResponse, Box<dyn std::error::Error>> {
    if output_languages.is_empty() {
        return Ok(TranslationResponse { en_translation: String::new(), fi_translation: String::new() });
    }

    let azure_translator_key = azure_translator_key();
    let azure_translator_url = azure_translator_url();
    let azure_region = azure_region();

    let mut headers = HeaderMap::new();
    headers.insert("Ocp-Apim-Subscription-Key", HeaderValue::from_str(azure_translator_key.as_str())?);
    headers.insert("Ocp-Apim-Subscription-Region", HeaderValue::from_str(azure_region.as_str())?);
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    headers.insert("User-Agent", HeaderValue::from_static("Reqwest"));

    let body = format!("[{{ \"Text\": \"{}\" }}]", text);
    let output_language = output_languages.iter().map(|lang| lang.to_string()).collect::<Vec<String>>().join(",");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}&to={}", azure_translator_url, output_language))
        .headers(headers)
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

async fn convert_text_to_speech_with_aws(text: &str, language: TextToSpeechLanguage) -> Result<(), Box<dyn std::error::Error>> {
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new("eu-central-1"))
        .load()
        .await;

    let polly_client = PollyClient::new(&config);
    let polly_response: SynthesizeSpeechOutput = polly_client
        .synthesize_speech()
        .engine(Engine::Neural)
        .text(text)
        .voice_id(VoiceId::Tomoko)
        // .voice_id(VoiceId::Suvi)
        .output_format(OutputFormat::Mp3)
        .language_code(LanguageCode::JaJp)
        // .language_code(LanguageCode::FiFi)
        .text_type(TextType::Text)
        .send()
        .await?;

    // Save the audio to a file
    let audio_stream = polly_response.audio_stream.collect().await?;
    println!("Output type: {}", polly_response.content_type.unwrap());
    let mut file = File::create("output_audio.mp3")?;

    file.write_all(audio_stream.to_vec().as_slice())?;

    Ok(())
}

async fn playback_sound() -> Result<(), Box<dyn std::error::Error>> {
    let soloud = Soloud::default()?;
    let mut wav = audio::Wav::default();
    let mut file = File::open("output_audio.mp3")?;
    let mut file_vector = Vec::new();
    file.read_to_end(&mut file_vector)?;
    wav.load_mem(file_vector.as_slice())?;
    soloud.play(&wav);
    while soloud.voice_count() > 0 {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
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