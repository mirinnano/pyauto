use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use std::io::{self, Write};

fn main() {
    loop {
        println!("========================================");
        println!("   ANTIGRAVITY ADMIN CONSOLE v1.0       ");
        println!("   [1] Generate NEW Master Keypair      ");
        println!("   [2] Sign User HWID (Create License)  ");
        println!("   [3] Exit                             ");
        println!("========================================");
        print!("> Select Option: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        match input.trim() {
            "1" => generate_keys(),
            "2" => sign_hwid(),
            "3" => break,
            _ => println!("Invalid option."),
        }

        println!("\nPress ENTER to return to menu...");
        let mut pause = String::new();
        io::stdin().read_line(&mut pause).unwrap();
    }
}

fn generate_keys() {
    println!("\n[GENERATING NEW MASTER KEYPAIR]...");
    use rand::RngCore;
    let mut csprng = OsRng;
    let mut bytes = [0u8; 32];
    csprng.fill_bytes(&mut bytes);

    let signing_key = SigningKey::from_bytes(&bytes);
    let verifying_key = VerifyingKey::from(&signing_key);

    let priv_b64 = BASE64.encode(signing_key.to_bytes());
    let pub_b64 = BASE64.encode(verifying_key.to_bytes());

    println!("\n!!! SAVE THESE KEYS SECURELY !!!");
    println!("--------------------------------------------------");
    println!("PRIVATE KEY (Keep Secret, use for Signing):");
    println!("{}", priv_b64);
    println!("--------------------------------------------------");
    println!("PUBLIC KEY (Embed in App 'src/engine/license.rs'):");
    println!("{}", pub_b64);
    println!("--------------------------------------------------");
}

fn sign_hwid() {
    println!("\n[LICENSE GENERATION]");

    // Hardcoded Master Key (As requested)
    let priv_b64 = "epm7+hYKHoSdQMsydFPoxmeo5ybk1rjH8WUWzh/ug/0=";
    println!("> Using Hardcoded Master Private Key");

    print!("> Enter User HWID: ");
    io::stdout().flush().unwrap();
    let mut hwid_in = String::new();
    io::stdin().read_line(&mut hwid_in).unwrap();
    let hwid = hwid_in.trim();

    // Decode Private Key
    let priv_bytes = match BASE64.decode(priv_b64) {
        Ok(b) => b,
        Err(e) => {
            println!("Error decoding Private Key: {}", e);
            return;
        }
    };

    let bytes: [u8; 32] = match priv_bytes.try_into() {
        Ok(b) => b,
        Err(_) => {
            println!("Invalid Private Key Length (Must be 32 bytes dec)");
            return;
        }
    };
    let signing_key = SigningKey::from_bytes(&bytes);

    // Sign
    let signature = signing_key.sign(hwid.as_bytes());
    let sig_b64 = BASE64.encode(signature.to_bytes());

    println!("\n[LICENSE GENERATED SUCCESSFULLY]");
    println!("--------------------------------------------------");
    println!("{}", sig_b64);
    println!("--------------------------------------------------");
    println!("Send this code to the user.");
}
