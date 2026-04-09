use ml_dsa::{
    MlDsa87, Signature, SigningKey, VerifyingKey,
    signature::{Signer, Verifier},
};

pub fn sign(signer: SigningKey<MlDsa87>, content: &[u8]) -> Vec<u8> {
    let signature = signer.sign(content);
    signature.encode().as_slice().to_vec()
}

pub fn verify_signature(
    verifier: VerifyingKey<MlDsa87>,
    content: &[u8],
    signature: &[u8],
) -> Result<(), ml_dsa::Error> {
    let mut sig_arr = ml_dsa::EncodedSignature::<MlDsa87>::default();
    sig_arr.as_mut_slice().copy_from_slice(signature);
    let signature = Signature::<MlDsa87>::decode(&sig_arr).unwrap();
    verifier.verify(content, &signature)
}

pub fn verify_key(key_source: &Vec<u8>) -> VerifyingKey<MlDsa87> {
    let mut key_arr = ml_dsa::EncodedVerifyingKey::<MlDsa87>::default();
    key_arr.as_mut_slice().copy_from_slice(key_source);
    VerifyingKey::decode(&key_arr)
}

pub fn signer_key(key_source: &Vec<u8>) -> SigningKey<MlDsa87> {
    let mut key_arr = ml_dsa::ExpandedSigningKey::<MlDsa87>::default();
    key_arr.as_mut_slice().copy_from_slice(key_source);
    SigningKey::from_expanded(&key_arr)
}
