use std::collections::{HashMap, HashSet};

use cashu::dhke::{sign_message, verify_message};
pub use cashu::error::mint::Error;
use cashu::nuts::{
    BlindedMessage, BlindedSignature, MeltBolt11Request, MeltBolt11Response, Proof, SwapRequest,
    SwapResponse, *,
};
#[cfg(feature = "nut07")]
use cashu::nuts::{CheckSpendableRequest, CheckSpendableResponse};
use cashu::secret::Secret;
use cashu::Amount;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::types::Quote;

pub struct Mint {
    //    pub pubkey: PublicKey
    secret: String,
    pub keysets: HashMap<Id, nut02::mint::KeySet>,
    pub keysets_info: HashMap<Id, MintKeySetInfo>,
    pub spent_secrets: HashSet<Secret>,
    pub pending_secrets: HashSet<Secret>,
    pub fee_reserve: FeeReserve,
    pub quotes: HashMap<String, Quote>,
}

impl Mint {
    pub fn new(
        secret: &str,
        keysets_info: HashSet<MintKeySetInfo>,
        spent_secrets: HashSet<Secret>,
        quotes: Vec<Quote>,
        min_fee_reserve: Amount,
        percent_fee_reserve: f32,
    ) -> Self {
        let mut keysets = HashMap::default();
        let mut info = HashMap::default();

        let mut active_units: HashSet<String> = HashSet::default();

        let quotes = quotes.into_iter().map(|q| (q.id.clone(), q)).collect();

        // Check that there is only one active keyset per unit
        for keyset_info in keysets_info {
            if keyset_info.active && !active_units.insert(keyset_info.unit.clone()) {
                // TODO: Handle Error
                todo!()
            }

            let keyset = nut02::mint::KeySet::generate(
                secret,
                keyset_info.unit.clone(),
                keyset_info.derivation_path.clone(),
                keyset_info.max_order,
            );

            keysets.insert(keyset.id, keyset);

            info.insert(keyset_info.id, keyset_info);
        }

        Self {
            secret: secret.to_string(),
            keysets,
            quotes,
            keysets_info: info,
            spent_secrets,
            pending_secrets: HashSet::new(),
            fee_reserve: FeeReserve {
                min_fee_reserve,
                percent_fee_reserve,
            },
        }
    }

    /// Retrieve the public keys of the active keyset for distribution to
    /// wallet clients
    pub fn keyset_pubkeys(&self, keyset_id: &Id) -> Option<KeysResponse> {
        let keys: Keys = match self.keysets.get(keyset_id) {
            Some(keyset) => keyset.keys.clone().into(),
            None => {
                return None;
            }
        };

        Some(KeysResponse { keys })
    }

    /// Return a list of all supported keysets
    pub fn keysets(&self) -> KeysetResponse {
        let keysets = self
            .keysets_info
            .values()
            .map(|k| k.clone().into())
            .collect();

        KeysetResponse { keysets }
    }

    pub fn keyset(&self, id: &Id) -> Option<KeySet> {
        self.keysets.get(id).map(|ks| ks.clone().into())
    }

    pub fn process_mint_request(
        &mut self,
        mint_request: nut04::MintBolt11Request,
    ) -> Result<nut04::MintBolt11Response, Error> {
        let mut blind_signatures = Vec::with_capacity(mint_request.outputs.len());

        for blinded_message in mint_request.outputs {
            blind_signatures.push(self.blind_sign(&blinded_message)?);
        }

        Ok(nut04::MintBolt11Response {
            signatures: blind_signatures,
        })
    }

    fn blind_sign(&self, blinded_message: &BlindedMessage) -> Result<BlindedSignature, Error> {
        let BlindedMessage {
            amount,
            b,
            keyset_id,
        } = blinded_message;

        let keyset = self.keysets.get(keyset_id).ok_or(Error::UnknownKeySet)?;

        // Check that the keyset is active and should be used to sign
        if !self
            .keysets_info
            .get(keyset_id)
            .ok_or(Error::UnknownKeySet)?
            .active
        {
            return Err(Error::InactiveKeyset);
        }

        let Some(key_pair) = keyset.keys.0.get(amount) else {
            // No key for amount
            return Err(Error::AmountKey);
        };

        let c = sign_message(key_pair.secret_key.clone().into(), b.clone().into())?;

        Ok(BlindedSignature {
            amount: *amount,
            c: c.into(),
            id: keyset.id,
        })
    }

    pub fn process_split_request(
        &mut self,
        split_request: SwapRequest,
    ) -> Result<SwapResponse, Error> {
        let proofs_total = split_request.input_amount();

        let output_total = split_request.output_amount();

        if proofs_total != output_total {
            return Err(Error::Amount);
        }

        let proof_count = split_request.inputs.len();

        let secrets: HashSet<Secret> = split_request
            .inputs
            .iter()
            .map(|p| p.secret.clone())
            .collect();

        // Check that there are no duplicate proofs in request
        if secrets.len().ne(&proof_count) {
            return Err(Error::DuplicateProofs);
        }

        for proof in &split_request.inputs {
            self.verify_proof(proof)?
        }

        for secret in secrets {
            self.spent_secrets.insert(secret);
        }

        let promises: Vec<BlindedSignature> = split_request
            .outputs
            .iter()
            .map(|b| self.blind_sign(b).unwrap())
            .collect();

        Ok(SwapResponse::new(promises))
    }

    fn verify_proof(&self, proof: &Proof) -> Result<(), Error> {
        if self.spent_secrets.contains(&proof.secret) {
            return Err(Error::TokenSpent);
        }

        let keyset = self.keysets.get(&proof.id).ok_or(Error::UnknownKeySet)?;

        let Some(keypair) = keyset.keys.0.get(&proof.amount) else {
            return Err(Error::AmountKey);
        };

        verify_message(
            keypair.secret_key.clone().into(),
            proof.c.clone().into(),
            &proof.secret,
        )?;

        Ok(())
    }

    #[cfg(feature = "nut07")]
    pub fn check_spendable(
        &self,
        check_spendable: &CheckSpendableRequest,
    ) -> Result<CheckSpendableResponse, Error> {
        let mut spendable = Vec::with_capacity(check_spendable.proofs.len());
        let mut pending = Vec::with_capacity(check_spendable.proofs.len());

        for proof in &check_spendable.proofs {
            spendable.push(!self.spent_secrets.contains(&proof.secret));
            pending.push(self.pending_secrets.contains(&proof.secret));
        }

        Ok(CheckSpendableResponse { spendable, pending })
    }

    pub fn verify_melt_request(&mut self, melt_request: &MeltBolt11Request) -> Result<(), Error> {
        let quote = self.quotes.get(&melt_request.quote).unwrap();
        let proofs_total = melt_request.proofs_amount().to_sat();

        let required_total = quote.amount + quote.fee_reserve;

        if proofs_total < required_total {
            debug!(
                "Insufficient Proofs: Got: {}, Required: {}",
                proofs_total, required_total
            );
            return Err(Error::Amount);
        }

        let secrets: HashSet<&Secret> = melt_request.inputs.iter().map(|p| &p.secret).collect();

        // Ensure proofs are unique and not being double spent
        if melt_request.inputs.len().ne(&secrets.len()) {
            return Err(Error::DuplicateProofs);
        }

        for proof in &melt_request.inputs {
            self.verify_proof(proof)?
        }

        Ok(())
    }

    pub fn process_melt_request(
        &mut self,
        melt_request: &MeltBolt11Request,
        preimage: &str,
        total_spent: Amount,
    ) -> Result<MeltBolt11Response, Error> {
        self.verify_melt_request(melt_request)?;

        let secrets = Vec::with_capacity(melt_request.inputs.len());
        for secret in secrets {
            self.spent_secrets.insert(secret);
        }

        let mut change = None;

        if let Some(outputs) = melt_request.outputs.clone() {
            let change_target = melt_request.proofs_amount() - total_spent;
            let mut amounts = change_target.split();
            let mut change_sigs = Vec::with_capacity(amounts.len());

            if outputs.len().lt(&amounts.len()) {
                debug!(
                    "Providing change requires {} blinded messages, but only {} provided",
                    amounts.len(),
                    outputs.len()
                );

                // In the case that not enough outputs are provided to return all change
                // Reverse sort the amounts so that the most amount of change possible is
                // returned. The rest is burnt
                amounts.sort_by(|a, b| b.cmp(a));
            }

            for (amount, blinded_message) in amounts.iter().zip(outputs) {
                let mut blinded_message = blinded_message;
                blinded_message.amount = *amount;

                let signature = self.blind_sign(&blinded_message)?;
                change_sigs.push(signature)
            }

            change = Some(change_sigs);
        } else {
            info!(
                "No change outputs provided. Burnt: {} sats",
                (melt_request.proofs_amount() - total_spent).to_sat()
            );
        }

        Ok(MeltBolt11Response {
            paid: true,
            proof: preimage.to_string(),
            change,
        })
    }
}

pub struct FeeReserve {
    pub min_fee_reserve: Amount,
    pub percent_fee_reserve: f32,
}

#[derive(Debug, Hash, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintKeySetInfo {
    pub id: Id,
    pub unit: String,
    pub active: bool,
    pub valid_from: u64,
    pub valid_to: Option<u64>,
    pub derivation_path: String,
    pub max_order: u8,
}

impl From<MintKeySetInfo> for KeySetInfo {
    fn from(keyset_info: MintKeySetInfo) -> Self {
        Self {
            id: keyset_info.id,
            unit: keyset_info.unit,
        }
    }
}
