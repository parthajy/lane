// lane-mcp: Lane as an MCP server for other local AI tools (read-only).
// Opens the user's vault with the key from their Keychain; serves on stdio.
fn main() {
    let data_dir = rat_mac_lib::data_dir();
    let db = data_dir.join("memory.db");
    if !db.is_file() {
        eprintln!("lane-mcp: no Lane database at {}", db.display());
        std::process::exit(1);
    }
    let store = match rat_mac_lib::store::Store::open_readonly(&db) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("lane-mcp: cannot open the vault: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = rat_mac_lib::mcp::serve(&store) {
        eprintln!("lane-mcp: {e}");
        std::process::exit(1);
    }
}
