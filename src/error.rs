use std::fmt;

use libp2p::identity::error::DecodingError as WalletDecodingError;

pub type Result<T> = std::result::Result<T, MarkChainError>;

type TransportIoError = libp2p::TransportError<std::io::Error>;

#[derive(Debug)]
pub enum MarkChainError {
    Transport(TransportIoError),
    Noise(libp2p::noise::NoiseError),
    Io(std::io::Error),
    Multiaddr(libp2p::core::multiaddr::Error),
    SerdeJson(serde_json::Error),
    Sled(sled::Error),
    WalletDecoding(WalletDecodingError),
}

impl fmt::Display for MarkChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarkChainError::Transport(error) => write!(f, "Transport: {} ", error),
            MarkChainError::Noise(error) => write!(f, "Noise: {} ", error),
            MarkChainError::Io(error) => write!(f, "Io: {} ", error),
            MarkChainError::Multiaddr(error) => write!(f, "Multiaddr: {}", error),
            MarkChainError::SerdeJson(error) => write!(f, "SerdeJson: {}", error),
            MarkChainError::Sled(error) => write!(f, "Sled: {}", error),
            MarkChainError::WalletDecoding(error) => write!(f, "WalletDecoding: {}", error),
        }
    }
}

impl std::error::Error for MarkChainError {}

impl From<TransportIoError> for MarkChainError {
    fn from(error: TransportIoError) -> MarkChainError {
        MarkChainError::Transport(error)
    }
}

impl From<libp2p::noise::NoiseError> for MarkChainError {
    fn from(error: libp2p::noise::NoiseError) -> MarkChainError {
        MarkChainError::Noise(error)
    }
}

impl From<std::io::Error> for MarkChainError {
    fn from(error: std::io::Error) -> MarkChainError {
        MarkChainError::Io(error)
    }
}

impl From<libp2p::core::multiaddr::Error> for MarkChainError {
    fn from(error: libp2p::core::multiaddr::Error) -> MarkChainError {
        MarkChainError::Multiaddr(error)
    }
}

impl From<serde_json::Error> for MarkChainError {
    fn from(error: serde_json::Error) -> MarkChainError {
        MarkChainError::SerdeJson(error)
    }
}

impl From<sled::Error> for MarkChainError {
    fn from(error: sled::Error) -> MarkChainError {
        MarkChainError::Sled(error)
    }
}

impl From<WalletDecodingError> for MarkChainError {
    fn from(error: WalletDecodingError) -> MarkChainError {
        MarkChainError::WalletDecoding(error)
    }
}
