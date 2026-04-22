use aes_gcm::{Aes256Gcm, KeyInit};
use ml_dsa::{KeyGen, KeyPair, MlDsa87};
use ml_kem::{
    ArraySize, B32, Ciphertext, FromSeed, KeyExport, MlKem1024, Seed, SharedKey,
    array::Array,
    kem::{Decapsulate, DecapsulationKey, EncapsulationKey},
};
use rand_crypto::{CryptoRng, Rng};

pub fn generate_exchange_keys() -> (DecapsulationKey<MlKem1024>, EncapsulationKey<MlKem1024>) {
    let mut rng = rand_crypto::rng();
    let mut seed = Seed::default();
    rng.fill_bytes(seed.as_mut_slice());
    let (dk, ek) = MlKem1024::from_seed(&seed);
    (dk, ek)
}

pub fn key_exchange(dec: &DecapsulationKey<MlKem1024>, cipher_text_vec: Vec<u8>) -> Vec<u8> {
    //let cipher_text = EncodedCiphertext::from(cipher_text_vec);
    let mut encapsulated_cipher_read = Array::default();
    encapsulated_cipher_read
        .as_mut_slice()
        .copy_from_slice(&cipher_text_vec);
    let decapsulated = dec.decapsulate(&encapsulated_cipher_read);
    decapsulated.as_slice().to_vec()
}

pub fn give_exchange(enc_str: &str) -> (String, SharedKey) {
    let mut rng = rand_crypto::rng();
    let shared: B32 = rand(&mut rng);
    let enc_vec = hex::decode(enc_str).unwrap();
    let mut encapsulated_key_read = Array::default();
    encapsulated_key_read
        .as_mut_slice()
        .copy_from_slice(&enc_vec);
    let enc = EncapsulationKey::<MlKem1024>::new(&encapsulated_key_read).expect("valid ek");
    let (encapsulated_cipher, shared_key): (Ciphertext<MlKem1024>, SharedKey) =
        enc.encapsulate_deterministic(&shared);
    (hex::encode(encapsulated_cipher.as_slice()), shared_key)
}

pub fn generate_signer_keys() -> KeyPair<MlDsa87> {
    let mut rng = rand_crypto::rng();
    let kp = MlDsa87::key_gen(&mut rng);
    kp
}

pub fn seed_aes_gcm(input: [u8; 32]) -> Aes256Gcm {
    Aes256Gcm::new((&input).into())
}

pub fn rand<L: ArraySize, R: CryptoRng + ?Sized>(rng: &mut R) -> Array<u8, L> {
    let mut val = Array::default();
    rng.fill_bytes(&mut val);
    val
}

#[test]
fn give_exchange_test() {
    let (dec, enc) = generate_exchange_keys();

    let enc_save = enc.to_bytes();
    let enc_str = hex::encode(&enc_save);

    let (ret, shared_key) = give_exchange(&enc_str);
    let shared_key_vec: Vec<u8> = shared_key.into_iter().collect();

    let ret_vec = hex::decode(ret).unwrap();

    let mut encapsulated_cipher_read = Array::default();
    encapsulated_cipher_read
        .as_mut_slice()
        .copy_from_slice(&ret_vec);

    let dec_shared_key = dec.decapsulate(&encapsulated_cipher_read);

    let shared_local_str = hex::encode(&shared_key_vec);
    let shared_decapsulated_str = hex::encode(&dec_shared_key);
    println!("{shared_local_str}");
    println!("{shared_decapsulated_str}");

    assert_eq!(shared_local_str, shared_decapsulated_str)
}

#[test]
fn kem_tut() {
    let (dec, enc) = generate_exchange_keys();
    let dec_save = dec.to_bytes();
    let enc_save = enc.to_bytes();

    println!("Decapsulation key");
    println!("{}", hex::encode(&dec_save));
    println!("len: {}", dec_save.len());

    println!("Encapsulation key");
    println!("{}", hex::encode(&dec_save));
    println!("len: {}", dec_save.len());

    let mut rng = rand_crypto::rng();
    let shared: B32 = rand(&mut rng);

    let (encapsulated_cipher, shared_key): (Ciphertext<MlKem1024>, SharedKey<MlKem1024>) =
        enc.encapsulate_deterministic(&shared);
    println!("Encapsulated cipher");
    println!("{}", hex::encode(&encapsulated_cipher));
    println!("len: {}", encapsulated_cipher.len());

    let cipher_vec = encapsulated_cipher.as_slice().to_vec();
    println!("Encapsulated cipher (byte converted)");
    println!("{}", hex::encode(&cipher_vec));
    println!("len: {}", cipher_vec.len());

    let mut encapsulated_cipher_read = Array::default();
    encapsulated_cipher_read
        .as_mut_slice()
        .copy_from_slice(&cipher_vec);

    println!("Encapsulated cipher (after byte conversion)");
    println!("{}", hex::encode(&encapsulated_cipher_read));
    println!("len: {}", encapsulated_cipher_read.len());

    println!("--- The following 4 value pairs should be the same ---");

    let shared_key_vec: Vec<u8> = shared_key.into_iter().collect();
    println!("Shared key (returned by encapsulation method)");
    println!("{}", hex::encode(&shared_key_vec));
    println!("len: {}", shared_key_vec.len());

    let dec_shared_key = dec.decapsulate(&encapsulated_cipher);
    println!("Decapsulated shared key");
    println!("{}", hex::encode(&dec_shared_key));
    println!("len: {}", dec_shared_key.len());

    let dec_shared_key2 = dec.decapsulate(&encapsulated_cipher_read);
    println!("Decapsulated shared key (after byte conversions)");
    println!("{}", hex::encode(&dec_shared_key2));
    println!("len: {}", dec_shared_key2.len());

    println!("Shared key (source)");
    println!("{}", hex::encode(&shared));
    println!("len: {}", shared.len());
}

#[test]
fn dsa_tut() {
    let lorem_ipsum = "#Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.#";
    let signer_kp = generate_signer_keys();
    let signer_key = signer_kp.signing_key();
    signer_key.encode();
    let verify_key = signer_kp.verifying_key();
    verify_key.encode();

    let signature = signer_key.sign(lorem_ipsum.as_bytes());
    verify_key
        .verify(lorem_ipsum.as_bytes(), &signature)
        .unwrap();
}
