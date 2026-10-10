//! Makes a corp portal account for the accounts file. FOR DEMOS AND TESTING ONLY (ADR 0025).
//!
//!   awzy-corp-user --username max --name "Max" < password.txt >> accounts entry
//!
//! Reads the password from the first line of stdin and prints the account as JSON with an
//! argon2id hash. Add it to the list in AWZY_CORP_PORTAL_ACCOUNTS_FILE and restart the corp portal.

use std::io::BufRead;

use anyhow::bail;
use awzy_corp_portal::auth::{hash_password, StaffAccount};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(username), Some(name)) = (arg(&args, "--username"), arg(&args, "--name")) else {
        bail!("usage: awzy-corp-user --username <username> --name <display name> < password");
    };
    let mut password = String::new();
    std::io::stdin().lock().read_line(&mut password)?;
    let password = password.trim_end_matches(['\r', '\n']);
    if password.chars().count() < 12 {
        bail!("use a password of at least 12 characters");
    }
    let account = StaffAccount { username, name, password_hash: hash_password(password)? };
    println!("{}", serde_json::to_string_pretty(&account)?);
    Ok(())
}
