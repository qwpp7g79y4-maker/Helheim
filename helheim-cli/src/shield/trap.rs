use rand::Rng;

pub struct DesireEngine;

impl DesireEngine {
    /// Generates a cursed .env file (The "Greed" Trap)
    pub fn generate_env() -> String {
        let mut rng = rand::rng();
        let aws_key: String = (0..20)
            .map(|_| rng.random_range(b'A'..=b'Z') as char)
            .collect();
        let aws_secret: String = (0..40)
            .map(|_| rng.random_range(b'a'..=b'z') as char)
            .collect();

        // Generate alphanumeric Stripe key
        let stripe_suffix: String = (0..24)
            .map(|_| {
                let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
                let idx = rng.random_range(0..chars.len());
                chars.chars().nth(idx).unwrap()
            })
            .collect();

        // Forge a Honey URL for the DB Host
        let honey_db = crate::shield::wire::Wire::forge_honey_url("db");

        format!(
            "# PRODUCTION CONFIG - DO NOT COMMIT\n\
            AWS_ACCESS_KEY_ID=AKIA{}\n\
            AWS_SECRET_ACCESS_KEY={}\n\
            DB_HOST={}\n\
            DB_USER=admin\n\
            DB_PASS=P@ssw0rd2026!\n\
            STRIPE_SECRET_KEY=sk_live_{}\n",
            aws_key, aws_secret, honey_db, stripe_suffix
        )
    }

    /// Generates a fake RSA Private Key (The "Power" Trap)
    pub fn generate_rsa() -> String {
        let mut rng = rand::rng();
        let body: String = (0..24)
            .map(|_| {
                let chunk: String = (0..64)
                    .map(|_| {
                        let chars =
                            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                        let idx = rng.random_range(0..64);
                        chars.chars().nth(idx).unwrap()
                    })
                    .collect();
                format!("{}\n", chunk)
            })
            .collect();

        format!(
            "-----BEGIN RSA PRIVATE KEY-----\n\
            {}\
            -----END RSA PRIVATE KEY-----",
            body
        )
    }

    /// Generates a fake SQL Dump (The "Knowledge" Trap)
    pub fn generate_sql() -> String {
        "-- Admin Users Dump (2026-02-01)\n\
        INSERT INTO `users` (`id`, `email`, `password_hash`, `is_admin`) VALUES\n\
        (1, 'root@helheim.local', '$2y$10$eO1.o/P/q', 1),\n\
        (2, 'ceo@company.com', '$2y$10$9X2.a/B/c', 1);\n\
        -- CRITICAL: DELETE THIS FILE AFTER IMPORT"
            .to_string()
    }
}
