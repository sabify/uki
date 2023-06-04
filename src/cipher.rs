use tokio::io::ReadBuf;

pub trait Encryption {
    fn encrypt(&self, buf: &mut ReadBuf);
    fn decrypt(&self, buf: &mut ReadBuf);
}

#[derive(Debug)]
pub enum Cipher {
    Xor(Vec<u8>),
}

impl TryFrom<&str> for Cipher {
    type Error = String;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        let input = input.to_lowercase();
        let input: Vec<&str> = input.splitn(2, ':').collect();
        match input[0] {
            "xor" => {
                if input[1].is_empty() {
                    Err("xor key should be provided. format: 'xor:<key>'".into())
                } else {
                    Ok(Self::Xor(input[1].into()))
                }
            }
            _ => Err(format!("{} encryption is not supported.", input[0])),
        }
    }
}

impl Encryption for Cipher {
    #[inline]
    fn encrypt(&self, input: &mut ReadBuf) {
        match self {
            Self::Xor(ref key) => {
                let mut key = key.iter().cycle();
                input.filled_mut().iter_mut().for_each(|x| {
                    *x ^= match key.next() {
                        Some(key) => key,
                        None => unreachable!(),
                    }
                });
            }
        }
    }

    #[inline]
    fn decrypt(&self, input: &mut ReadBuf) {
        match self {
            Self::Xor(_) => {
                self.encrypt(input);
            }
        }
    }
}
