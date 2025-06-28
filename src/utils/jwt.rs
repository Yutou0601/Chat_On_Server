use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey,
                   Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims { pub sub:String, pub exp:i64 }

pub fn sign(uid:&str, secret:&str) -> String {
    let c = Claims { sub: uid.into(), exp: Utc::now().timestamp()+86_400 };
    encode(&Header::default(), &c, &EncodingKey::from_secret(secret.as_bytes()))
        .unwrap()
}

pub fn verify(token:&str, secret:&str) -> Option<String> {
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()),
                     &Validation::new(Algorithm::HS256))
        .map(|d| d.claims.sub).ok()
}
