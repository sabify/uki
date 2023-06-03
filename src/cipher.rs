#[derive(Debug, Clone)]
pub enum Encryption {
    Xor(Vec<u8>),
}

impl From<String> for Encryption {
    fn from(input: String) -> Self {
        Self::from(input.as_str())
    }
}

impl From<&String> for Encryption {
    fn from(input: &String) -> Self {
        Self::from(input.as_str())
    }
}

impl From<&str> for Encryption {
    fn from(input: &str) -> Self {
        let input = input.to_lowercase();
        let input: Vec<&str> = input.splitn(2, ':').collect();
        match input[0] {
            "xor" => {
                if input.len() < 2 {
                    panic!("xor key should be provided");
                } else {
                    Self::Xor(input[1].into())
                }
            }
            _ => {
                panic!("{} encryption is not supported.", input[0]);
            }
        }
    }
}

impl Encryption {
    #[inline]
    pub fn encrypt(&self, input: &mut [u8]) {
        match self {
            Self::Xor(ref key) => {
                let mut key = key.iter().cycle();
                input.iter_mut().for_each(|x| {
                    *x ^= match key.next() {
                        Some(key) => key,
                        None => unreachable!(),
                    }
                });
            }
        }
    }

    #[inline]
    pub fn decrypt(&self, input: &mut [u8]) {
        match self {
            Self::Xor(_) => {
                self.encrypt(input);
            }
        }
    }
}
