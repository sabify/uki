pub trait Encryptor {
    fn encrypt(&self, buf: &mut [u8]);
    fn decrypt(&self, buf: &mut [u8]);
}

#[derive(Debug, Default)]
pub enum Cipher {
    #[default]
    Plain,
    Xor(Vec<u8>),
}

impl TryFrom<&str> for Cipher {
    type Error = String;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        if input.is_empty() {
            return Ok(Self::Plain);
        }

        let (method, data) = if let Some((method, data)) = input.split_once(':') {
            (method, data)
        } else {
            return Err("needs two parts, 'method:data'".into());
        };

        match method {
            "xor" => {
                if data.is_empty() {
                    Err("xor key should be provided. format: 'xor:<key>'".into())
                } else {
                    Ok(Self::Xor(data.into()))
                }
            }
            _ => Err(format!("{method} encryption is not supported.")),
        }
    }
}

impl Encryptor for Cipher {
    #[inline]
    fn decrypt(&self, input: &mut [u8]) {
        match self {
            Self::Plain => {}
            Self::Xor(key) => {
                for (byte, &key_byte) in input.iter_mut().zip(key.iter().cycle()) {
                    *byte ^= key_byte;
                }
            }
        }
    }

    #[inline]
    fn encrypt(&self, input: &mut [u8]) {
        match self {
            Self::Plain => {}
            Self::Xor(key) => {
                for (byte, &key_byte) in input.iter_mut().zip(key.iter().cycle()) {
                    *byte ^= key_byte;
                }
            }
        }
    }
}
