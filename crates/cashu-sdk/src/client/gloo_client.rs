//! gloo wasm http Client

use async_trait::async_trait;
#[cfg(feature = "nut09")]
use cashu::nuts::MintInfo;
use cashu::nuts::{
    BlindedMessage, Keys, MeltBolt11Request, MeltBolt11Response, MintBolt11Request,
    MintBolt11Response, PreMintSecrets, Proof, SwapRequest, SwapResponse, *,
};
#[cfg(feature = "nut07")]
use cashu::nuts::{CheckSpendableRequest, CheckSpendableResponse};
use gloo::net::http::Request;
use serde_json::Value;
use url::Url;

use super::join_url;
use crate::client::{Client, Error};

#[derive(Debug, Clone)]
pub struct HttpClient {}

#[async_trait(?Send)]
impl Client for HttpClient {
    /// Get Mint Keys [NUT-01]
    async fn get_mint_keys(&self, mint_url: Url) -> Result<Keys, Error> {
        let url = join_url(mint_url, "keys")?;
        let keys = Request::get(url.as_str())
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let keys: Keys = serde_json::from_str(&keys.to_string())?;
        Ok(keys)
    }

    /// Get Keysets [NUT-02]
    async fn get_mint_keysets(&self, mint_url: Url) -> Result<KeysetResponse, Error> {
        let url = join_url(mint_url, "keysets")?;
        let res = Request::get(url.as_str())
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<KeysetResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Mint Tokens [NUT-04]
    async fn post_mint(
        &self,
        mint_url: Url,
        quote: &str,
        premint_secrets: PreMintSecrets,
    ) -> Result<MintBolt11Response, Error> {
        let url = join_url(mint_url, "mint")?;

        let request = MintBolt11Request {
            quote: quote.to_string(),
            outputs: premint_secrets.blinded_messages(),
        };

        let res = Request::post(url.as_str())
            .json(&request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<MintBolt11Response, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Melt [NUT-05]
    /// [Nut-08] Lightning fee return if outputs defined
    async fn post_melt(
        &self,
        mint_url: Url,
        quote: String,
        inputs: Vec<Proof>,
        outputs: Option<Vec<BlindedMessage>>,
    ) -> Result<MeltBolt11Response, Error> {
        let url = join_url(mint_url, "melt")?;

        let request = MeltBolt11Request {
            quote,
            inputs,
            outputs,
        };

        let value = Request::post(url.as_str())
            .json(&request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<MeltBolt11Response, serde_json::Error> =
            serde_json::from_value(value.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&value.to_string())?),
        }
    }

    /// Split Token [NUT-06]
    async fn post_split(
        &self,
        mint_url: Url,
        split_request: SwapRequest,
    ) -> Result<SwapResponse, Error> {
        let url = join_url(mint_url, "split")?;

        let res = Request::post(url.as_str())
            .json(&split_request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<SwapResponse, serde_json::Error> = serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Spendable check [NUT-07]
    #[cfg(feature = "nut07")]
    async fn post_check_spendable(
        &self,
        mint_url: Url,
        proofs: Vec<nut00::mint::Proof>,
    ) -> Result<CheckSpendableResponse, Error> {
        let url = join_url(mint_url, "check")?;
        let request = CheckSpendableRequest {
            proofs: proofs.to_owned(),
        };

        let res = Request::post(url.as_str())
            .json(&request)
            .map_err(|err| Error::Gloo(err.to_string()))?
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<CheckSpendableResponse, serde_json::Error> =
            serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }

    /// Get Mint Info [NUT-09]
    #[cfg(feature = "nut09")]
    async fn get_mint_info(&self, mint_url: Url) -> Result<MintInfo, Error> {
        let url = join_url(mint_url, "info")?;
        let res = Request::get(url.as_str())
            .send()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| Error::Gloo(err.to_string()))?;

        let response: Result<MintInfo, serde_json::Error> = serde_json::from_value(res.clone());

        match response {
            Ok(res) => Ok(res),
            Err(_) => Err(Error::from_json(&res.to_string())?),
        }
    }
}
