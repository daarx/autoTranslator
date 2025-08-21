pub mod camera_capture {
    use std::fs::File;
    use std::io::Read;
    use opencv::core::{Mat, Rect};
    use opencv::prelude::{MatExprTraitConst, MatTraitConst, VideoCaptureTrait, VideoCaptureTraitConst};
    use opencv::videoio::VideoCapture;
    use crate::threshold;

    pub struct CameraCapture {
        cap: VideoCapture,
        height: i32,
        width: i32,
    }

    impl CameraCapture {
        pub fn new(width: i32, height: i32) -> Self {
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

        pub fn capture_image(
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
}