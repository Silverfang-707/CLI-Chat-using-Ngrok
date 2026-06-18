use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();

    println!("========================================");
    println!("          AIRAA DECENTRALIZED           ");
    println!("========================================");
    println!("1) Host a Node & Chat (Creates Network)");
    println!("2) Join a Node (Connects to Network)");
    print!("> Select operation: ");
    io::stdout().flush()?;

    let mut choice = String::new();
    stdin.read_line(&mut choice)?;

    match choice.trim() {
        "1" => host_and_chat().await?,
        "2" => join_network().await?,
        _ => println!("Invalid operation. Terminating."),
    }

    Ok(())
}

fn find_exe(name: &str) -> Option<String> {
    if Command::new(name).arg("--version").output().is_ok() {
        return Some(name.to_string());
    }

    let finder = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(out) = Command::new(finder).arg(name).output() {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path.lines().next().unwrap_or(name).to_string());
        }
    }
    None
}

fn get_bundled_tor() -> Option<std::path::PathBuf> {
    let mut exe_dir = env::current_exe().ok()?;
    exe_dir.pop();
    let tor_exe = exe_dir
        .join("Tor Browser")
        .join("Browser")
        .join("TorBrowser")
        .join("Tor")
        .join(format!("tor{}", env::consts::EXE_SUFFIX));

    if tor_exe.exists() { Some(tor_exe) } else { None }
}

async fn host_and_chat() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();

    print!("\nInitialize local port (Press Enter for 3000): ");
    io::stdout().flush()?;
    let mut port = String::new();
    stdin.read_line(&mut port)?;
    let port = if port.trim().is_empty() { "3000" } else { port.trim() }.to_string();

    // 1. START SERVER FIRST
    let mut exe_dir = env::current_exe()?; exe_dir.pop();
    let server_exe = exe_dir.join(format!("server{}", env::consts::EXE_SUFFIX));
    
    println!("Starting background server node...");
    let mut server_child = Command::new(&server_exe)
        .arg(&port)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    sleep(Duration::from_millis(500)).await;

    // 2. CONFIGURE ROUTING
    println!("\n=== Select Uplink Route ===");
    println!("1) Local Network Only\n2) Ngrok Tunnel\n3) Tor Hidden Service");
    print!("> Choice: ");
    io::stdout().flush()?;
    let mut route_choice = String::new();
    stdin.read_line(&mut route_choice)?;

    let mut background_child = None;
    let mut uplink_url = "Local".to_string();

    if route_choice.trim() == "2" {
        let ngrok_exe = match find_exe("ngrok") {
            Some(exe) => exe,
            None => {
                println!("❌ Ngrok not found.");
                print!("Install automatically via Winget? (y/n): ");
                io::stdout().flush()?;
                let mut install = String::new(); stdin.read_line(&mut install)?;
                if install.trim().to_lowercase() != "y" {
                    let _ = server_child.kill();
                    return Ok(());
                }
                
                println!("Installing Ngrok...");
                if !Command::new("winget")
                    .args(["install", "Ngrok.Ngrok", "--accept-source-agreements", "--accept-package-agreements"])
                    .status()?.success() 
                {
                    println!("❌ Failed to install Ngrok.");
                    let _ = server_child.kill();
                    return Ok(());
                }
                
                sleep(Duration::from_secs(5)).await;
                match find_exe("ngrok") {
                    Some(exe) => exe,
                    None => {
                        println!("Ngrok installed. Please restart AIRAA to refresh PATH.");
                        let _ = server_child.kill();
                        return Ok(());
                    }
                }
            }
        };

        println!("Spawning Ngrok tunnel...");
        let child = Command::new(&ngrok_exe).args(["tcp", &port]).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        sleep(Duration::from_secs(3)).await;

        let client = reqwest::Client::new();
        if let Ok(resp) = client.get("http://127.0.0.1:4040/api/tunnels").send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(url) = json["tunnels"].as_array().and_then(|t| t.first()).and_then(|t| t["public_url"].as_str()) {
                    uplink_url = url.replace("tcp://", "");
                    println!("🌐 PUBLIC UPLINK: {}", uplink_url);
                }
            }
        }
        background_child = Some(child);

    } else if route_choice.trim() == "3" {
        let tor_exe = match get_bundled_tor() {
            Some(path) => path,
            None => {
                println!("❌ Tor Browser not found beside AIRAA.");
                let _ = server_child.kill();
                return Ok(());
            }
        };

        let tor_dir = exe_dir.join("airaa_tor_config");
        let hs_dir = tor_dir.join("hidden_service");
        fs::create_dir_all(&hs_dir)?;
        
        let hs_dir_abs = fs::canonicalize(&hs_dir).unwrap_or(hs_dir);
        let torrc_path = tor_dir.join("torrc");
        // FIX: virtual port must be 80, matching the default port the
        // client assumes when a user enters a bare ".onion" address
        // (see client.rs: `if !server_addr.contains(':') { ... :80 }`).
        // It was previously 9001, which meant Tor had no route for any
        // connection arriving on the default port 80, so all incoming
        // Tor connections silently failed.
        let torrc_content = format!(
            "SocksPort 9050\n\
             HiddenServiceDir {}\n\
             HiddenServicePort 80 127.0.0.1:{}\n",
            hs_dir_abs.display(),
            port
        );
        fs::write(&torrc_path, torrc_content)?;

        // LOGGING FIX: Redirect Tor logs to file instead of console
        let tor_log = fs::File::create("airaa_tor.log")?;
        
        println!("Spawning Tor Hidden Service (Logs stored in airaa_tor.log)...");
        let child = Command::new(&tor_exe)
            .current_dir(tor_exe.parent().unwrap())
            .args(["-f", torrc_path.to_str().unwrap()])
            .stdout(Stdio::from(tor_log.try_clone()?))
            .stderr(Stdio::from(tor_log))
            .spawn()?;
        background_child = Some(child);

        let hostname_file = hs_dir_abs.join("hostname");
        for _ in 0..60 {
            if let Ok(onion) = fs::read_to_string(&hostname_file) {
                if !onion.trim().is_empty() {
                    uplink_url = onion.trim().to_string();
                    println!("========================================");
                    println!("🧅 ONION UPLINK: {}", uplink_url);
                    println!("========================================");
                    break;
                }
            }
            sleep(Duration::from_secs(1)).await;
        }
    }

    // 3. LAUNCH UI (Pass URL as argument)
    println!("Launching client UI...");
    let client_exe = exe_dir.join(format!("client{}", env::consts::EXE_SUFFIX));
    let mut client_child = Command::new(&client_exe)
        .arg(format!("127.0.0.1:{}", port))
        .arg(&uplink_url) // NEW: Pass the uplink URL to the client
        .spawn()?;
    let _ = client_child.wait();

    // 4. CLEANUP
    let _ = server_child.kill();
    if let Some(mut daemon) = background_child { let _ = daemon.kill(); }
    
    Ok(())
}

async fn join_network() -> Result<(), Box<dyn std::error::Error>> {
    let mut exe_dir = env::current_exe()?; exe_dir.pop();
    let client_exe = exe_dir.join(format!("client{}", env::consts::EXE_SUFFIX));
    let _ = Command::new(&client_exe).spawn()?.wait();
    Ok(())
}