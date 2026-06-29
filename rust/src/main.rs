#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::hashes::Hash;
use bitcoincore_rpc::bitcoin::{Address, Amount, BlockHash, OutPoint, Transaction, TxIn, Txid};
use bitcoincore_rpc::json::GetMempoolEntryResult;
use bitcoincore_rpc::Error::ReturnedError;
use bitcoincore_rpc::{Auth, Client, Error as RpcError, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fmt;
use std::fs::{File, OpenOptions};
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

// Creates a wallet if needed, loads it, then returns an rpc for that specific wallet
fn prepare_wallet_rpc(rpc: &Client, wallet_name: &str) -> Result<Client, bitcoincore_rpc::Error> {
    let available_wallets = rpc.list_wallet_dir()?;
    let loaded_wallets = rpc.list_wallets()?;
    if !available_wallets.contains(&wallet_name.to_string()) {
        // Create wallet
        rpc.create_wallet(wallet_name, None, None, None, None)?;
    } else if !loaded_wallets.contains(&wallet_name.to_owned()) {
        // Wallet exists already, but not loaded yet. Load it
        rpc.load_wallet(wallet_name)?;
    }

    let auth = Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned());
    let wallet_rpc_url = format!("{}/wallet/{}", RPC_URL, wallet_name);
    Client::new(&wallet_rpc_url, auth.clone())
}

// Prepares Miner and Trader wallets
// Returns tuple: (miner_wallet_rpc: Client, trader_wallet_rpc: Client)
fn prepate_test_wallet_rpcs(rpc: &Client) -> Result<(Client, Client), bitcoincore_rpc::Error> {
    let miner_wallet_rpc = prepare_wallet_rpc(rpc, WALLET_MINER)?;
    let trader_wallet_rpc = prepare_wallet_rpc(rpc, WALLET_TRADER)?;
    Ok((miner_wallet_rpc, trader_wallet_rpc))
}

fn main() -> bitcoincore_rpc::Result<()> {
    let auth = Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned());
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(RPC_URL, auth.clone())?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    // Create or load miner/trader wallets and get their respective rpcs
    let (miner_rpc, trader_rpc) = prepate_test_wallet_rpcs(&rpc)?;

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    let miner_address = miner_rpc
        .get_new_address(
            Some("first miner receive"),
            // Generate SegWit address
            Some(bitcoincore_rpc::json::AddressType::Bech32),
        )?
        // We could assume network is checked as it was generated
        // but let's make sure by requiring regtest
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        // Manually map error
        .map_err(|e| bitcoincore_rpc::Error::ReturnedError(e.to_string()))?;

    // Generate to address with 101 blocks
    // Need 100 after first coinbase for it to become spendable
    miner_rpc.generate_to_address(101, &miner_address)?;

    let trader_address = trader_rpc
        .get_new_address(
            Some("first trader receive"),
            // Generate SegWit address
            Some(bitcoincore_rpc::json::AddressType::Bech32),
        )?
        // Same as before: let's make sure by requiring regtest
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        // Manually map error
        .map_err(|e| ReturnedError(e.to_string()))?;

    // Send 20 BTC from Miner to Trader
    let send_amt: f64 = 20.0;
    let txid: Txid = miner_rpc.send_to_address(
        &trader_address,
        Amount::from_btc(send_amt)?,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    // Check transaction in mempool
    // Calling get_mempool_entry will fail if txid is not in there
    rpc.get_mempool_entry(&txid).map_err(|e| {
        // print details here to make it more obvious
        println!("Cannot find txid {} in mempool: {}", txid, e);

        // then return initial error
        e
    })?;

    // Mine 1 block to confirm the transaction
    miner_rpc.generate_to_address(1, &miner_address)?;

    // Extract all required transaction details
    let tx_raw_info = rpc.get_raw_transaction_info(&txid, None)?;

    // Get block hash
    let block_hash = tx_raw_info
        .blockhash
        .ok_or(ReturnedError(String::from("Block not confirmed")))?;

    // Get block info that contains height for a given hash
    let block_info = rpc.get_block_info(&block_hash)?;

    // Construct builder and populate with initial info
    let mut tx_info_builder = TxInfo::builder()
        .txid(txid)
        .block_hash(block_hash)
        .block_height(block_info.height as u32);

    // A bit weird because the test expects exactly one coinbase transaction to be used as input
    // So abort earlier if that is not the case
    let tx: Transaction = tx_raw_info.transaction()?;
    if tx.input.len() != 1 {
        return Err(ReturnedError(String::from(
            "Test expects exactly one (coinbase) input only",
        )));
    }

    // Unwrap first() as we checked input len() before
    let tx_in: &TxIn = tx.input.first().unwrap();
    if let Ok(prev_tx) = rpc.get_raw_transaction(&tx_in.previous_output.txid, None) {
        let prev_out = &prev_tx.output[tx_in.previous_output.vout as usize];

        let address = Address::from_script(
            &prev_out.script_pubkey,
            bitcoincore_rpc::bitcoin::Network::Regtest,
        )
        .map_err(|e| ReturnedError(e.to_string()))?;

        tx_info_builder = tx_info_builder.input(AddressAmount {
            address: address.clone(),
            amount: prev_out.value,
        });
    }

    // Address/amount out and change are in the out vector
    // If not 2 outputs, something is wrong, return error
    if tx.output.len() != 2 {
        return Err(ReturnedError(String::from("Test expects two outputs")));
    }

    // loop over outputs and match output vs change data
    for output in &tx.output {
        let addr = Address::from_script(
            &output.script_pubkey,
            bitcoincore_rpc::bitcoin::Network::Regtest,
        )
        .map_err(|e| ReturnedError(e.to_string()))?;

        if addr == trader_address {
            // Goes to trader's output address
            tx_info_builder = tx_info_builder.output(AddressAmount {
                address: addr.clone(),
                amount: output.value,
            });
        } else {
            // Goes to miner's change address
            tx_info_builder = tx_info_builder.change(AddressAmount {
                address: addr.clone(),
                amount: output.value,
            });
        }
    }

    // Build TxInfo helper struct
    let tx_info = tx_info_builder
        .build()
        .map_err(|e| ReturnedError(e.to_string()))?;

    // Write the data to ../out.txt in the specified format given in readme.md
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("../out.txt")?;

    let mut writer = BufWriter::new(file);
    writeln!(writer, "{}", tx_info)?;
    writer.flush()?;

    Ok(())
}
