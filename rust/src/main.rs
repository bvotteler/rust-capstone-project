use bitcoin::{blockdata::transaction::Transaction, Address, Amount, Network, TxIn, Txid};
use corepc_client::client_sync::{
    v24::{AddressType, Client},
    Auth, Error, Result as ClientResult,
};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};

pub mod tx_info;
use crate::tx_info::{AddressAmount, TxInfo};

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// Use wallet names the test expects
const WALLET_MINER: &str = "Miner";
const WALLET_TRADER: &str = "Trader";

// Creates a wallet if needed, loads it, then returns an client for that specific wallet
fn prepare_wallet_rpc(client: &Client, wallet_name: &str) -> ClientResult<Client> {
    let available_wallets = client.list_wallet_dir()?.wallets;
    let loaded_wallets = client.list_wallets()?.0;
    if !available_wallets
        .iter()
        .any(|wallet| wallet.name == wallet_name)
    {
        // No wallet found in directory, create it
        client.create_wallet(wallet_name)?;
    } else if !loaded_wallets.contains(&wallet_name.to_owned()) {
        // Wallet exists already, but not loaded yet. Load it
        client.load_wallet(wallet_name)?;
    }

    let auth = Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned());
    let wallet_rpc_url = format!("{}/wallet/{}", RPC_URL, wallet_name);
    Client::new_with_auth(&wallet_rpc_url, auth.clone())
}

// Prepares Miner and Trader wallets
// Returns tuple: (miner_client: Client, trader_client: Client)
fn prepate_test_wallet_rpcs(client: &Client) -> ClientResult<(Client, Client)> {
    let miner_client = prepare_wallet_rpc(client, WALLET_MINER)?;
    let trader_client = prepare_wallet_rpc(client, WALLET_TRADER)?;
    Ok((miner_client, trader_client))
}

fn main() -> ClientResult<()> {
    let auth = Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned());
    // Connect to Bitcoin Core RPC
    let client = Client::new_with_auth(RPC_URL, auth)?;

    // Get blockchain info
    let blockchain_info = client.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    // Create or load miner/trader wallets and get their respective corepc clients
    let (miner_client, trader_client) = prepate_test_wallet_rpcs(&client)?;

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    // First: Generate address to receive coinbase rewards to.
    let miner_address = miner_client
        .get_new_address(
            Some("first miner receive"),
            // Generate SegWit address
            Some(AddressType::Bech32),
        )?
        .address()
        // Map parse error into client's Returned Error
        .map_err(|e| Error::Returned(e.to_string()))?
        // We could assume network is checked as it was generated
        // but let's make sure by requiring regtest
        .require_network(Network::Regtest)
        // Map error
        .map_err(|e| Error::Returned(e.to_string()))?;

    // Generate to address with 101 blocks
    // Need 100 after first coinbase for it to become spendable
    miner_client.generate_to_address(101, &miner_address)?;

    // Generate receiving address for trader
    let trader_address = trader_client
        .get_new_address(Some("first trader receive"), Some(AddressType::Bech32))?
        .address()
        .map_err(|e| Error::Returned(e.to_string()))?
        .require_network(Network::Regtest)
        .map_err(|e| Error::Returned(e.to_string()))?;

    // Send 20 BTC from Miner to Trader
    let send_amount = Amount::from_int_btc(20);
    let txid: Txid = miner_client
        .send_to_address(&trader_address, send_amount)?
        .txid()?;

    // Check transaction in mempool.
    // Calling get_mempool_entry will fail if txid is not in there.
    client.get_mempool_entry(txid).map_err(|e| {
        // Print details here to make it more obvious
        println!("Cannot find txid {} in mempool: {}", txid, e);

        // Then return initial error
        e
    })?;

    // Mine 1 block to confirm the transaction
    miner_client.generate_to_address(1, &miner_address)?;

    // Extract all required transaction details.
    let tx_raw_info = client
        .get_raw_transaction_verbose(txid)?
        // Call into_model() for typed output instead of Strings and primitive types
        .into_model()
        .map_err(|e| Error::Returned(e.to_string()))?;

    let block_hash = tx_raw_info
        .block_hash
        .ok_or(Error::Returned(String::from("Block not confirmed")))?;

    // Get block info that contains height for a given hash
    let block_info = client.get_block_verbose_one(block_hash)?;

    // Construct TxInfoBuilder and populate with initial data
    let mut tx_info_builder = TxInfo::builder()
        .txid(txid)
        .block_hash(block_hash)
        .block_height(block_info.height as u32);

    // A bit weird because the test expects exactly one coinbase transaction to be used as input
    // So abort earlier if that is not the case
    let tx: Transaction = tx_raw_info.transaction;
    if tx.input.len() != 1 {
        return Err(Error::Returned(String::from(
            "Test expects exactly one (coinbase) input only",
        )));
    }

    // Unwrap first() as we checked input len() before
    let tx_in: &TxIn = tx.input.first().unwrap();
    if let Ok(prev_tx) = client.get_raw_transaction(tx_in.previous_output.txid) {
        let prev_out = &prev_tx
            .transaction()
            .map_err(|e| Error::Returned(e.to_string()))?
            .output[tx_in.previous_output.vout as usize]
            .clone();

        let address = Address::from_script(&prev_out.script_pubkey, Network::Regtest)
            .map_err(|e| Error::Returned(e.to_string()))?;

        // Add input data to builder
        tx_info_builder = tx_info_builder.input(AddressAmount {
            address: address.clone(),
            amount: prev_out.value,
        });
    }

    // Address/amount out and change are in the out vector
    // If not 2 outputs, something is wrong, return error
    if tx.output.len() != 2 {
        return Err(Error::Returned(String::from("Test expects two outputs")));
    }

    // loop over outputs and match output vs change data
    for output in &tx.output {
        let addr = Address::from_script(&output.script_pubkey, Network::Regtest)
            .map_err(|e| Error::Returned(e.to_string()))?;

        if addr == trader_address {
            // Goes to trader's output address, add data to builder
            tx_info_builder = tx_info_builder.output(AddressAmount {
                address: addr.clone(),
                amount: output.value,
            });
        } else {
            // Goes to miner's change address, add data to builder
            tx_info_builder = tx_info_builder.change(AddressAmount {
                address: addr.clone(),
                amount: output.value,
            });
        }
    }

    // Construct TxInfo from builder
    let tx_info = tx_info_builder
        .build()
        .map_err(|e| Error::Returned(e.to_string()))?;

    // Write the data to ../out.txt in the specified format given in readme.md
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("../out.txt")?;

    // Construct BufWriter and write to file
    let mut writer = BufWriter::new(file);
    writeln!(writer, "{}", tx_info)?;
    writer.flush()?;

    Ok(())
}
