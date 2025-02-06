use std::fs::File;

use aws_config::BehaviorVersion;
use aws_sdk_polly::operation::synthesize_speech::SynthesizeSpeechOutput;
use aws_sdk_polly::types::{Engine, LanguageCode, OutputFormat, TextType, VoiceId};
use aws_sdk_polly::Client as PollyClient;
use aws_types::region::Region;
use nokhwa::pixel_format::{LumaFormat, RgbFormat, YuyvFormat};
use nokhwa::utils::{CameraIndex, ControlValueSetter, KnownCameraControl, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::multipart;
use std::io::{Read, Write};
use std::process::exit;
use std::vec;
use soloud::{AudioExt, LoadExt, audio, Soloud};
use tokio;

const AZURE_OCR_URL: &str = "AZURE_OCR_URL";
const AZURE_TEXT_TO_SPEECH_URL: &str = "AZURE_TEXT_TO_SPEECH_URL";
const AZURE_OCR_KEY: &str = "AZURE_OCR_KEY";
const AZURE_TEXT_TO_SPEECH_KEY: &str = "AZURE_TEXT_TO_SPEECH_KEY";
const USE_AWS_TEXT_TO_SPEECH: &str = "USE_AWS_TEXT_TO_SPEECH";

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok(); // Load AWS and Azure credentials from file into environment variables

    use text_io::read;

    println!("Press enter to capture, q and enter to quit:");
    let mut line: String = read!("{}\n");

    while !line.contains("q") {
        capture_process_playback().await;

        println!("Press enter to capture, q and enter to quit:");
        line = read!("{}\n");
    }
}

async fn capture_process_playback() {
    match capture_image_from_webcam() {
        Ok(image_buffer) => match extract_text_from_image(image_buffer).await {
            Ok(extracted_text) => {
                println!("extracted text from image: {}", extracted_text);
                let text_to_speech_result = if dotenv::var(USE_AWS_TEXT_TO_SPEECH).unwrap().trim().parse().unwrap() { convert_text_to_speech_with_aws(extracted_text).await } else { convert_text_to_speech_with_azure(extracted_text).await };

                match text_to_speech_result {
                    Ok(()) => {
                        if let Err(e) = playback_sound() {
                            eprintln!("Error when playing back sound: {}", e);
                        }
                    },
                    Err(e) => eprintln!("Failed to convert text: {}", e),
                }
            }
            Err(e) => {
                eprintln!("Could not extract text from image: {}", e)
            }
        },
        Err(error) => {
            eprintln!("Could not load image: {}", error)
        }
    }
}

fn capture_image_from_webcam() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // first camera in system
    let index = CameraIndex::Index(0);
    // request the absolute highest resolution CameraFormat that can be decoded to RGB.
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::HighestResolution(Resolution::new(1920, 1080)));
    // make the camera
    let mut camera = Camera::new(index, requested)?;

    // camera.compatible_camera_formats()?.iter().for_each(|format| {
    //     println!("Format: {:?}", format);
    // });

    // camera.camera_controls_known_camera_controls()?.iter().for_each(|control| {
    //     println!("Control: {:?}", control);
    //
    //     if let KnownCameraControl::Other(10094861) = control.0 {
    //         println!("Setting Zoom");
    //         camera.set_camera_control(*control.0, ControlValueSetter::Integer(1)).expect("Could not set camera control");
    //     }
    //
    //     if let KnownCameraControl::Brightness = control.0 {
    //         println!("Setting Zoom");
    //         camera.set_camera_control(*control.0, ControlValueSetter::Integer(10)).expect("Could not set camera control");
    //     }
    // });

    camera.open_stream()?;

    // get a frame
    camera.frame()?;
    camera.frame()?;
    camera.frame()?;
    camera.frame()?;
    let frame = camera.frame()?;
    println!("Captured Single Frame of {}", frame.buffer().len());
    // decode into an ImageBuffer
    let decoded = frame.decode_image::<RgbFormat>()?;
    println!("Decoded Frame of {}", decoded.len());

    decoded.save("output_image.jpg")?;

    let mut output_file = File::open("output_image.jpg")?;
    let mut output_vec = Vec::new();
    output_file.read_to_end(&mut output_vec)?;

    // exit(0);

    Ok(output_vec)
}

async fn extract_text_from_image(buffer: Vec<u8>) -> Result<String, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Ocp-Apim-Subscription-Key",
        HeaderValue::from_str(dotenv::var(AZURE_OCR_KEY)?.as_str())?,
    );

    let part = multipart::Part::bytes(buffer).mime_str("image/jpg")?;
    let form = multipart::Form::new().part("file", part);

    let client = reqwest::Client::new();
    let response = client
        .post(dotenv::var(AZURE_OCR_URL)?)
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

    println!("Response: {}", response);

    Ok(output)
}

async fn convert_text_to_speech_with_azure(text: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Ocp-Apim-Subscription-Key",
        HeaderValue::from_str(dotenv::var(AZURE_TEXT_TO_SPEECH_KEY)?.as_str())?,
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

    let body = format!("<speak version='1.0' xml:lang='ja-JP'><voice xml:lang='ja-JP' xml:gender='Female' name='ja-JP-NanamiNeural'>{}</voice></speak>", text);

    let client = reqwest::Client::new();
    let response = client
        .post(dotenv::var(AZURE_TEXT_TO_SPEECH_URL)?)
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

async fn convert_text_to_speech_with_aws(text: String) -> Result<(), Box<dyn std::error::Error>> {
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

fn playback_sound() -> Result<(), Box<dyn std::error::Error>> {
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
