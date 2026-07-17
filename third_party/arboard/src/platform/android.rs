use crate::common::Error;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[cfg(feature = "image-data")]
use crate::common::ImageData;

pub(crate) struct Clipboard;

impl Clipboard {
    pub fn new() -> Result<Self, Error> {
        Ok(Self)
    }
}

pub(crate) struct Get<'clipboard> {
    _platform: &'clipboard mut Clipboard,
}

impl<'clipboard> Get<'clipboard> {
    pub fn new(platform: &'clipboard mut Clipboard) -> Self {
        Self { _platform: platform }
    }

    pub fn text(self) -> Result<String, Error> {
        // Run termux-clipboard-get if available
        let output = std::process::Command::new("termux-clipboard-get")
            .output();
        match output {
            Ok(out) if out.status.success() => {
                let s = String::from_utf8_lossy(&out.stdout).into_owned();
                Ok(s)
            }
            _ => {
                Ok(String::new())
            }
        }
    }

    #[cfg(feature = "image-data")]
    pub fn image(self) -> Result<ImageData<'static>, Error> {
        Err(Error::ContentNotAvailable)
    }

    pub fn html(self) -> Result<String, Error> {
        Err(Error::ContentNotAvailable)
    }

    pub fn file_list(self) -> Result<Vec<PathBuf>, Error> {
        Err(Error::ContentNotAvailable)
    }
}

pub(crate) struct Set<'clipboard> {
    _platform: &'clipboard mut Clipboard,
}

impl<'clipboard> Set<'clipboard> {
    pub fn new(platform: &'clipboard mut Clipboard) -> Self {
        Self { _platform: platform }
    }

    pub fn text(self, text: Cow<'_, str>) -> Result<(), Error> {
        let mut child = std::process::Command::new("termux-clipboard-set")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::Unknown { description: e.to_string() })?;
        
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).ok();
        }
        child.wait().ok();
        Ok(())
    }

    pub fn html(self, _html: Cow<'_, str>, _alt_text: Option<Cow<'_, str>>) -> Result<(), Error> {
        Ok(())
    }

    #[cfg(feature = "image-data")]
    pub fn image(self, _image: ImageData) -> Result<(), Error> {
        Ok(())
    }

    pub fn file_list(self, _file_list: &[impl AsRef<Path>]) -> Result<(), Error> {
        Ok(())
    }
}

pub(crate) struct Clear<'clipboard> {
    _platform: &'clipboard mut Clipboard,
}

impl<'clipboard> Clear<'clipboard> {
    pub fn new(platform: &'clipboard mut Clipboard) -> Self {
        Self { _platform: platform }
    }

    pub fn clear(self) -> Result<(), Error> {
        std::process::Command::new("termux-clipboard-set")
            .arg("")
            .output()
            .ok();
        Ok(())
    }
}
