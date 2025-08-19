use std::cmp::Ordering;
use std::fmt::Display;
use std::fs::File;

use crate::TextToSpeechLanguage::{English, Finnish, Japanese, Swedish};
use base64::prelude::*;
use opencv::core::Rect;
use opencv::prelude::*;
use opencv::videoio::VideoCapture;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::multipart;
use serde_json::json;
use soloud::{audio, AudioExt, LoadExt, Soloud};
use std::io::{Read, Write};
use std::process::exit;
use std::str::FromStr;
use std::vec;
use tokio;

enum TextToSpeechLanguage {
    Japanese,
    English,
    Finnish,
    Swedish,
}

impl Display for TextToSpeechLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Japanese => f.write_str("ja"),
            English => f.write_str("en"),
            Finnish => f.write_str("fi"),
            Swedish => f.write_str("sv"),
        }
    }
}

struct TranslationResponse {
    en_translation: String,
    fi_translation: String,
    sv_translation: String,
}

struct UsageOptions {
    playback_en: bool,
    playback_fi: bool,
    use_translation: bool,
    half_screen: bool,
    debug_printing: bool,
    color_correction: bool,
}

struct CameraCapture {
    cap: VideoCapture,
    height: i32,
    width: i32,
}

impl CameraCapture {
    fn new(width: i32, height: i32) -> Self {
        let mut camera_capture = CameraCapture {
            cap: VideoCapture::new(0, CameraCapture::get_backend()).unwrap(),
            height,
            width,
        };

        if !camera_capture.cap.is_opened().unwrap() {
            return camera_capture;
        }

        camera_capture
            .cap
            .set(opencv::videoio::CAP_PROP_FRAME_WIDTH, width as f64)
            .unwrap();
        camera_capture
            .cap
            .set(opencv::videoio::CAP_PROP_FRAME_HEIGHT, height as f64)
            .unwrap();

        camera_capture.height = camera_capture
            .cap
            .get(opencv::videoio::CAP_PROP_FRAME_HEIGHT)
            .unwrap() as i32;
        camera_capture.width = camera_capture
            .cap
            .get(opencv::videoio::CAP_PROP_FRAME_WIDTH)
            .unwrap() as i32;

        camera_capture
    }

    fn capture_image(
        &mut self,
        half_screen: bool,
        color_correction: bool,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut mat = Mat::default();

        if !self.cap.read(&mut mat).unwrap() {
            panic!("Could not capture image!");
        }

        if half_screen {
            mat = self.get_cropped_image(mat)?;
        }

        if color_correction {
            mat = self.get_color_corrected_image(mat)?;
        }

        opencv::imgcodecs::imwrite_def("output_image.jpg", &mat).unwrap();

        let mut file = File::open("output_image.jpg")?;
        let mut bytes_vector = Vec::new();
        file.read_to_end(&mut bytes_vector)?;
        Ok(bytes_vector)
    }

    fn load_image_from_file(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let image_mat = opencv::imgcodecs::imread_def("test_image.jpg")?;
        let image_mat = self.get_color_corrected_image(image_mat)?;

        opencv::imgcodecs::imwrite_def("output_image.jpg", &image_mat).unwrap();

        let mut file = File::open("output_image.jpg")?;
        let mut bytes_vector = Vec::new();
        file.read_to_end(&mut bytes_vector)?;
        Ok(bytes_vector)
    }

    fn get_cropped_image(&mut self, mat: Mat) -> Result<Mat, Box<dyn std::error::Error>> {
        let crop_rect = Rect::new(0, self.height / 2, self.width, self.height / 2);
        let cropped_mat = mat.roi(crop_rect)?;
        Ok(cropped_mat.clone_pointee())
    }

    fn isolate_white_text(&mut self, gray: Mat) -> Result<Mat, Box<dyn std::error::Error>> {
        // let mut blurred = Mat::default();
        // opencv::imgproc::gaussian_blur(&gray, &mut blurred, opencv::core::Size::new(5, 5), 0.0, 0.0, opencv::core::BorderTypes::BORDER_CONSTANT as i32, opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT)?;
        //
        // // Apply threshold: pixels brighter than 200 become 255 (white), others become 0 (black)
        // let mut thresh = Mat::default();
        // opencv::imgproc::threshold(&blurred, &mut thresh, 200.0, 255.0, opencv::imgproc::THRESH_BINARY)?;
        //
        // // Optional: clean up noise with morphology
        // let mut morph = Mat::default();
        // let kernel = opencv::imgproc::get_structuring_element(
        //     opencv::imgproc::MORPH_RECT,
        //     opencv::core::Size::new(3, 3),
        //     opencv::core::Point::new(-1, -1),
        // )?;
        // opencv::imgproc::morphology_ex(&thresh, &mut morph, opencv::imgproc::MORPH_OPEN, &kernel, opencv::core::Point::new(-1, -1), 1, opencv::core::BORDER_CONSTANT, opencv::core::Scalar::default())?;

        // let result = self.blur(gray, 5, 5, 0.0, 0.0)?;
        let result = self.threshold(gray, threshold())?;
        // let result = self.morph(result, 3, 3)?;

        Ok(result)
    }

    fn blur(
        &mut self,
        source: Mat,
        k_width: i32,
        k_height: i32,
        sigma_x: f64,
        sigma_y: f64,
    ) -> Result<Mat, Box<dyn std::error::Error>> {
        let mut blurred = Mat::default();
        opencv::imgproc::gaussian_blur(
            &source,
            &mut blurred,
            opencv::core::Size::new(k_width, k_height),
            sigma_x,
            sigma_y,
            opencv::core::BorderTypes::BORDER_CONSTANT as i32,
            opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
        )?;
        Ok(blurred)
    }

    fn threshold(
        &mut self,
        source: Mat,
        threshold: f64,
    ) -> Result<Mat, Box<dyn std::error::Error>> {
        // Apply threshold: pixels brighter than 200 become 255 (white), others become 0 (black)
        let mut thresh = Mat::default();
        opencv::imgproc::threshold(
            &source,
            &mut thresh,
            threshold,
            255.0,
            opencv::imgproc::THRESH_BINARY,
        )?;
        Ok(thresh)
    }

    fn morph(
        &mut self,
        source: Mat,
        width: i32,
        height: i32,
    ) -> Result<Mat, Box<dyn std::error::Error>> {
        // Optional: clean up noise with morphology
        let mut morph = Mat::default();
        let kernel = opencv::imgproc::get_structuring_element(
            opencv::imgproc::MORPH_RECT,
            opencv::core::Size::new(width, height),
            opencv::core::Point::new(-1, -1),
        )?;
        opencv::imgproc::morphology_ex(
            &source,
            &mut morph,
            opencv::imgproc::MORPH_OPEN,
            &kernel,
            opencv::core::Point::new(-1, -1),
            1,
            opencv::core::BORDER_CONSTANT,
            opencv::core::Scalar::default(),
        )?;
        Ok(morph)
    }

    fn get_color_corrected_image(&mut self, mat: Mat) -> Result<Mat, Box<dyn std::error::Error>> {
        let mut grayscale_image = Mat::zeros_size(mat.size()?, mat.typ())?.to_mat()?;

        opencv::imgproc::cvt_color(
            &mat,
            &mut grayscale_image,
            opencv::imgproc::COLOR_BGR2GRAY,
            0,
            opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
        )?;

        let converted_image = self.isolate_white_text(grayscale_image)?;

        Ok(converted_image)
    }

    fn get_backend() -> i32 {
        if cfg!(target_os = "windows") {
            opencv::videoio::CAP_DSHOW
        } else {
            opencv::videoio::CAP_ANY
        }
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
            headers,
        }
    }

    async fn make_request(
        &self,
        buffer: Vec<u8>,
        usage_options: &UsageOptions,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let part = multipart::Part::bytes(buffer).mime_str("image/jpg")?;
        let form = multipart::Form::new().part("file", part);

        let response = self
            .client
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
        let mut interpreted_lines = Vec::with_capacity(5);
        if let Some(regions) = response["regions"].as_array() {
            for region in regions {
                for line in region["lines"].as_array().unwrap_or(&vec![]) {
                    let mut interpreted_line =
                        InterpretedLine::from_str(line["boundingBox"].as_str().unwrap()).unwrap();
                    let words: Vec<String> = line["words"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|w| w["text"].as_str().unwrap_or("").trim().to_string())
                        .collect();
                    interpreted_line.text.push_str(&words.join(""));
                    interpreted_lines.push(interpreted_line);
                }
            }
        } else {
            println!("No text detected.");
        }

        interpreted_lines.sort();

        let mut first_line_is_name = false;
        if interpreted_lines.len() > 1 {
            let first_line = interpreted_lines.first().unwrap();
            let mut rest_of_the_lines = interpreted_lines.iter().skip(1);

            if rest_of_the_lines.all(|line| first_line.x - line.x > 60) {
                first_line_is_name = true;
            }
        }

        if first_line_is_name {
            let name = &interpreted_lines.first().unwrap().text;

            output.push_str(name.as_str());
            output.push_str(": ");
            interpreted_lines
                .iter()
                .skip(1)
                .map(|line| line.text.as_str())
                .for_each(|line| output.push_str(line));
        } else {
            interpreted_lines
                .iter()
                .map(|line| line.text.as_str())
                .for_each(|line| output.push_str(&line));
        }

        Ok(output)
    }
}

struct GoogleOcrClient {
    client: reqwest::Client,
    headers: HeaderMap,
    token: String,
}

impl GoogleOcrClient {
    fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-goog-user-project",
            HeaderValue::from_str(google_project().as_str()).unwrap(),
        );
        headers.insert(
            "Content-Type",
            HeaderValue::from_str("application/json; charset=utf-8").unwrap(),
        );

        let token = if cfg!(target_os = "windows") {
            String::from_utf8(std::process::Command::new("cmd")
                .args(["/C", "gcloud", "auth", "print-access-token"])
                .output()
                .unwrap()
                .stdout).unwrap()
        } else {
            String::from_utf8(std::process::Command::new("sh")
                .args(["-c", "gcloud", "auth", "print-access-token"])
                .output()
                .unwrap()
                .stdout).unwrap()
        };

        println!("Google OCR token: {}", token.as_str());

        Self {
            client: reqwest::Client::new(),
            headers,
            token,
        }
    }

    async fn make_request(
        &self,
        buffer: Vec<u8>,
        usage_options: &UsageOptions,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let encoded_buffer = BASE64_URL_SAFE.encode(&buffer);

        let request = json!({
            "requests": [{
                "image": { "content": encoded_buffer },
                "features": [{ "type": "TEXT_DETECTION" }]
            }]
        });

        let json_response = self
            .client
            .post("https://vision.googleapis.com/v1/images:annotate")
            .headers(self.headers.clone())
            .bearer_auth(self.token.trim())
            .json(&request)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        // println!("GoogleOcrClient {:#?}", response);

        let mut extracted_text = String::with_capacity(100);
        if let Some(responses) = json_response["responses"].as_array() {
            responses.iter().for_each(|response| {
                if let Some(full_annotation) = response["fullTextAnnotation"]["text"].as_str() {
                    extracted_text.push_str(full_annotation);
                }
            });
        }

        Ok(extracted_text)
    }
}

#[derive(PartialEq, Eq)]
struct InterpretedLine {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    text: String,
}

impl InterpretedLine {
    fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            text: String::with_capacity(50),
        }
    }
}

impl FromStr for InterpretedLine {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split_result = s.split(',');

        if let (Some(x), Some(y), Some(width), Some(height), None) = (
            split_result.next(),
            split_result.next(),
            split_result.next(),
            split_result.next(),
            split_result.next(),
        ) {
            Ok(Self::new(
                x.parse::<i32>().map_err(|_| ())?,
                y.parse::<i32>().map_err(|_| ())?,
                width.parse::<i32>().map_err(|_| ())?,
                height.parse::<i32>().map_err(|_| ())?,
            ))
        } else {
            Err(())
        }
    }
}

impl Ord for InterpretedLine {
    fn cmp(&self, other: &Self) -> Ordering {
        // order by descending increasing y, then increasing x
        if self.y > other.y {
            Ordering::Greater
        } else if self.y < other.y {
            Ordering::Less
        } else if self.x > other.x {
            Ordering::Greater
        } else if self.x < other.x {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }
}

impl PartialOrd for InterpretedLine {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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
            headers,
        }
    }

    async fn make_request(
        &self,
        text: &String,
        language: TextToSpeechLanguage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let body = match language {
            Japanese => format!("<speak version='1.0' xml:lang='ja-JP'><voice xml:lang='ja-JP' xml:gender='Female' name='ja-JP-NanamiNeural'>{}</voice></speak>", text),
            English => format!("<speak version='1.0' xml:lang='en-US'><voice xml:lang='en-US' xml:gender='Female' name='en-US-AvaMultilingualNeural'>{}</voice></speak>", text),
            Finnish => format!("<speak version='1.0' xml:lang='fi-FI'><voice xml:lang='fi-FI' xml:gender='Female' name='fi-FI-SelmaNeural'>{}</voice></speak>", text),
            Swedish => format!("<speak version='1.0' xml:lang='fi-FI'><voice xml:lang='sv-SV' xml:gender='Female' name='sv-SV-SelmaNeural'>{}</voice></speak>", text),
        };

        let response = self
            .client
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
        headers.insert(
            "Ocp-Apim-Subscription-Key",
            HeaderValue::from_str(azure_translator_key.as_str()).unwrap(),
        );
        headers.insert(
            "Ocp-Apim-Subscription-Region",
            HeaderValue::from_str(azure_region.as_str()).unwrap(),
        );
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("User-Agent", HeaderValue::from_static("Reqwest"));

        Self {
            client: reqwest::Client::new(),
            headers,
        }
    }
    async fn make_request(
        &self,
        text: &String,
        output_languages: &[TextToSpeechLanguage],
    ) -> Result<TranslationResponse, Box<dyn std::error::Error>> {
        if output_languages.is_empty() {
            return Ok(TranslationResponse {
                en_translation: String::new(),
                fi_translation: String::new(),
                sv_translation: String::new(),
            });
        }

        let azure_translator_url = azure_translator_url();
        let body = format!("[{{ \"Text\": \"{}\" }}]", text);
        let output_language = output_languages
            .iter()
            .map(|lang| lang.to_string())
            .collect::<Vec<String>>()
            .join(",");

        let response = self
            .client
            .post(format!("{}&to={}", azure_translator_url, output_language))
            .headers(self.headers.clone())
            .body(body)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let mut translation_response = TranslationResponse {
            en_translation: String::new(),
            fi_translation: String::new(),
            sv_translation: String::new(),
        };

        match response[0]["translations"].as_array() {
            Some(translations) => {
                translations.iter().for_each(|translation| {
                    if translation["to"].as_str().unwrap_or("en") == "fi" {
                        translation_response.fi_translation = translation["text"].to_string();
                    } else if translation["to"].as_str().unwrap_or("en") == "en" {
                        translation_response.en_translation = translation["text"].to_string();
                    } else if translation["to"].as_str().unwrap_or("en") == "sv" {
                        translation_response.sv_translation = translation["text"].to_string();
                    }
                });
            }
            None => {
                println!("Did not get translations");
            }
        }

        Ok(translation_response)
    }
}

struct AudioPlayer {
    player: Soloud,
}

impl AudioPlayer {
    fn new() -> Self {
        Self {
            player: Soloud::default().unwrap(),
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
    let google_ocr_client = GoogleOcrClient::new();
    let text_to_speech_client = TextToSpeechClient::new();
    let translator_client = TranslatorClient::new();
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
            &ocr_client,
            &google_ocr_client,
            &text_to_speech_client,
            &translator_client,
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
    ocr_client: &OcrClient,
    google_ocr_client: &GoogleOcrClient,
    text_to_speech_client: &TextToSpeechClient,
    translator_client: &TranslatorClient,
    audio_player: &AudioPlayer,
    usage_options: &UsageOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let image_buffer = if use_test_file().parse()? {
        load_image_from_disk()?
    } else {
        camera.capture_image(usage_options.half_screen, usage_options.color_correction)?
    };

    let extracted_text = google_ocr_client
        .make_request(image_buffer, &usage_options)
        .await?;

    // let extracted_text = ocr_client
    //     .make_request(image_buffer, &usage_options)
    //     .await?;

    println!("Extracted text JP: {}", &extracted_text);

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
        translator_client.make_request(&extracted_text, languages.as_slice());

    text_to_speech_client
        .make_request(&extracted_text, Japanese)
        .await?;
    audio_player.play_audio("output_audio.mp3").await?;

    let translated_text = translated_text_future.await?;

    if !translated_text.en_translation.is_empty() {
        println!("Translated text EN: {}", &translated_text.en_translation);
    }

    if !translated_text.en_translation.is_empty() && usage_options.playback_en {
        text_to_speech_client
            .make_request(&translated_text.en_translation, English)
            .await?;
        audio_player.play_audio("output_audio.mp3").await?;
    }

    if !translated_text.fi_translation.is_empty() {
        println!("Translated text FI: {}", &translated_text.fi_translation);
    }

    if !translated_text.fi_translation.is_empty() && usage_options.playback_fi {
        text_to_speech_client
            .make_request(&translated_text.fi_translation, Finnish)
            .await?;
        audio_player.play_audio("output_audio.mp3").await?;
    }

    if !translated_text.sv_translation.is_empty() {
        println!("Translated text SV: {}", &translated_text.sv_translation);
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

fn google_project() -> String {
    dotenv::var("GOOGLE_PROJECT")
        .expect("Couldn't find GOOGLE_PROJECT")
        .parse()
        .unwrap()
}