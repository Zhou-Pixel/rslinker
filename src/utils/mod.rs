pub mod chat;

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::task::{self, Poll};
use std::sync::Arc;
use anyhow::Context;
use fast_async_mutex::RwLock;
use rustls::{PrivateKey, Certificate};


pub type ARwLock<T> = Arc<RwLock<T>>;


pub struct Never;

impl Never {
    #[inline]
    pub const fn never() -> Self {
        Never {}
    }
}

impl Future for Never {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut task::Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}


pub async fn load_private_key(path: impl AsRef<Path> + Copy) -> anyhow::Result<PrivateKey> {
    let key = tokio::fs::read(path).await.context("fail to read private_key")?;
    let key = if path.as_ref().extension().map_or(false, |x| x == "der") {
        rustls::PrivateKey(key)
    } else {
        let pkcs8 = rustls_pemfile::pkcs8_private_keys(&mut &*key)
            .context("malformed PKCS #8 private key")?;
        match pkcs8.into_iter().next() {
            Some(x) => rustls::PrivateKey(x),
            None => {
                let rsa = rustls_pemfile::rsa_private_keys(&mut &*key)
                    .context("malformed PKCS #1 private key")?;
                match rsa.into_iter().next() {
                    Some(x) => rustls::PrivateKey(x),
                    None => {
                        anyhow::bail!("no private keys found");
                    }
                }
            }
        }
    };

    Ok(key)
}


pub async fn load_certs(path: impl AsRef<Path> + Copy) -> anyhow::Result<Vec<Certificate>> {
    let cert = tokio::fs::read(path).await?;

    let cert_chain = if path.as_ref().extension().map_or(false, |x| x == "der") {
        vec![rustls::Certificate(cert)]
    } else {
        rustls_pemfile::certs(&mut &*cert)
            .context("invalid PEM-encoded certificate")?
            .into_iter()
            .map(rustls::Certificate)
            .collect()
    };
    Ok(cert_chain)
}
