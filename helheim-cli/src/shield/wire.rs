use rand::Rng;

pub struct Wire;

impl Wire {
    /// Generates a unique Canary Token ID
    pub fn generate_token() -> String {
        let mut rng = rand::rng();
        (0..16).map(|_| {
            let chars = "abcdef0123456789";
            let idx = rng.random_range(0..chars.len());
            chars.chars().nth(idx).unwrap()
        }).collect()
    }

    /// Wraps a token in a credible "Honey URL"
    /// In a real operation, this would point to our C2 server.
    /// For now, we use a placeholder that looks scary/tracking-like.
    pub fn forge_honey_url(service: &str) -> String {
        let token = Self::generate_token();
        match service {
            "slack" => format!("https://hooks.slack.com/services/T00000000/B00000000/{}", token),
            "db" => format!("db-{}.cluster-ro-xy7.eu-west-1.rds.amazonaws.com", token),
            "s3" => format!("https://s3.amazonaws.com/company-backups-secure/{}", token),
            _ => format!("https://api.internal.corp/v1/auth/callback?token={}", token),
        }
    }
}
