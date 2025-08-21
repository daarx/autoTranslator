pub mod audio_player {
    use std::fs::File;
    use std::io::Read;
    use soloud::{audio, AudioExt, LoadExt, Soloud};

    pub struct AudioPlayer {
        player: Soloud,
    }

    impl AudioPlayer {
        pub fn new() -> Self {
            Self {
                player: Soloud::default().unwrap(),
            }
        }

        pub async fn play_audio(&self, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
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
}
