pub mod utils {
    use std::cmp::Ordering;
    use std::fmt::Display;
    use std::str::FromStr;
    use TextToSpeechLanguage::{Japanese, English, Finnish, Swedish};

    pub enum TextToSpeechLanguage {
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

    pub struct TranslationResponse {
        pub en_translation: String,
        pub fi_translation: String,
        pub sv_translation: String,
    }

    pub struct UsageOptions {
        pub playback_en: bool,
        pub playback_fi: bool,
        pub use_translation: bool,
        pub half_screen: bool,
        pub debug_printing: bool,
        pub color_correction: bool,
    }

    #[derive(PartialEq, Eq)]
    pub struct InterpretedLine {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
        pub text: String,
    }

    impl InterpretedLine {
        pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
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
}