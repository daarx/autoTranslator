pub mod azure_clients {
    use std::fs::File;
    use std::io::Write;
    use std::str::FromStr;
    use reqwest::header::{HeaderMap, HeaderValue};
    use reqwest::multipart;
    use crate::{azure_ocr_key, azure_ocr_url, azure_region, azure_text_to_speech_key, azure_text_to_speech_url, azure_translator_key, azure_translator_url, UsageOptions};
    use crate::utils::utils::{InterpretedLine, TextToSpeechLanguage, TranslationResponse};
    use crate::utils::utils::TextToSpeechLanguage::{English, Finnish, Japanese, Swedish};

    pub struct AzureOcrClient {
        client: reqwest::Client,
        headers: HeaderMap,
    }

    impl AzureOcrClient {
        pub fn new() -> Self {
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

        pub async fn make_request(
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

    pub struct AzureTextToSpeechClient {
        client: reqwest::Client,
        headers: HeaderMap,
    }

    impl AzureTextToSpeechClient {
        pub fn new() -> Self {
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

        pub async fn make_request(
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

    pub struct AzureTranslatorClient {
        client: reqwest::Client,
        headers: HeaderMap,
    }

    impl AzureTranslatorClient {
        pub fn new() -> Self {
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
        pub async fn make_request(
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
}