//! Read-only native acceptance aid: does not execute tools, read their credentials,
//! authorize, write configuration, or make network/model requests.
fn main() -> anyhow::Result<()> {
    for entry in tokensmarket_core::inventory::ToolInventory::current().list()? {
        println!(
            "{}: {}",
            entry.name,
            if entry.installed {
                "installed"
            } else {
                "missing"
            }
        );
    }
    Ok(())
}
