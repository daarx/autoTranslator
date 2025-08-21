pub mod google_client {
    use crate::{UsageOptions};
    use base64::prelude::*;
    use reqwest::header::{HeaderMap, HeaderValue};
    use reqwest::Client;
    use serde_json::json;
    use std::fs::File;
    use std::io::Write;
    use std::process::Command;
    use crate::utils::utils::{TextToSpeechLanguage, TranslationResponse};

    pub struct GoogleCloudClient {
        client: Client,
        headers: HeaderMap,
        token: String,
    }

    impl GoogleCloudClient {
        pub fn new() -> Self {
            let token = if cfg!(target_os = "windows") {
                String::from_utf8(
                    Command::new("cmd")
                        .args(["/C", "gcloud", "auth", "print-access-token"])
                        .output()
                        .unwrap()
                        .stdout,
                )
                .unwrap()
            } else {
                String::from_utf8(
                    Command::new("sh")
                        .args(["-c", "gcloud", "auth", "print-access-token"])
                        .output()
                        .unwrap()
                        .stdout,
                )
                .unwrap()
            };

            let gcloud_config = if cfg!(target_os = "windows") {
                String::from_utf8(
                    Command::new("cmd")
                        .args(["/C", "gcloud", "config", "list"])
                        .output()
                        .unwrap()
                        .stdout,
                )
                .unwrap()
            } else {
                String::from_utf8(
                    Command::new("sh")
                        .args(["-c", "gcloud", "config", "list"])
                        .output()
                        .unwrap()
                        .stdout,
                )
                .unwrap()
            };

            let project = GoogleCloudClient::extract_google_project(gcloud_config.as_str())
                .unwrap()
                .trim();

            let mut headers = HeaderMap::new();
            headers.insert(
                "x-goog-user-project",
                HeaderValue::from_str(project).unwrap(),
            );
            headers.insert(
                "Content-Type",
                HeaderValue::from_str("application/json; charset=utf-8").unwrap(),
            );

            Self {
                client: Client::new(),
                headers,
                token,
            }
        }

        pub async fn make_ocr_request(
            &self,
            buffer: Vec<u8>,
            usage_options: &UsageOptions,
        ) -> Result<String, Box<dyn std::error::Error>> {
            let encoded_buffer = BASE64_STANDARD.encode(&buffer);

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

        pub async fn make_tts_request(
            &self,
            text: &String,
            language: TextToSpeechLanguage,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let request = json!({
                "input": {
                    "markup": text
                },
                "voice": {
                    "languageCode": "ja-JP",
                    "name": "ja-JP-Chirp3-HD-Achernar",
                    "voiceClone": {}
                },
                "audioConfig": {
                    "audioEncoding": "MP3"
                }
            });

            let json_response = self
                .client
                .post("https://texttospeech.googleapis.com/v1/text:synthesize")
                .headers(self.headers.clone())
                .bearer_auth(self.token.trim())
                .json(&request)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            if let Some(audio_content) = json_response["audioContent"].as_str() {
                let decoded_audio = BASE64_STANDARD.decode(audio_content.as_bytes())?;

                // Save the audio to a file
                let mut file =
                    File::create("output_audio.mp3").expect("Failed to create audio file");
                let _ = file
                    .write_all(decoded_audio.as_slice())
                    .expect("Failed to write to file");
            }

            Ok(())
        }

        pub async fn make_trans_request(
            &self,
            text: &String,
            output_languages: &[TextToSpeechLanguage],
        ) -> Result<TranslationResponse, Box<dyn std::error::Error>> {
            let request = json!({
                "q": text,
                "source": "ja",
                "target": "en",
                "format": "text"
            });

            let json_response = self
                .client
                .post("https://translation.googleapis.com/language/translate/v2")
                .headers(self.headers.clone())
                .bearer_auth(self.token.trim())
                .json(&request)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            let mut cum_translation = String::with_capacity(100);
            if let Some(translations) = json_response["data"]["translations"].as_array() {
                translations.iter().for_each(|translation| {
                    if let Some(translated_text) = translation["translatedText"].as_str() {
                        cum_translation.push_str(translated_text);
                        cum_translation.push('\n');
                    }
                });
            }

            Ok(TranslationResponse {
                en_translation: cum_translation,
                fi_translation: "".to_string(),
                sv_translation: "".to_string(),
            })
        }

        fn extract_google_project(config: &str) -> Option<&str> {
            config
                .find("project = ")
                .map(|project_start| config.split_at(project_start + 10).1)
        }
    }
}
