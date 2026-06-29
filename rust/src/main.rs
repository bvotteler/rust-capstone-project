#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::{Address, Amount, OutPoint, Transaction, TxIn, Txid};
use bitcoincore_rpc::json::GetMempoolEntryResult;
use bitcoincore_rpc::Error::ReturnedError;
use bitcoincore_rpc::{Auth, Client, Error as RpcError, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// use wallet names the test expects
const WALLET_MINER: &str = "Miner";
const WALLET_TRADER: &str = "Trader";

// creates a wallet if needed, then returns an rpc for that specific wallet
fn prepare_wallet_rpc(rpc: &Client, wallet_name: &str) -> Result<(Client), bitcoincore_rpc::Error> {
    let available_wallets = rpc.list_wallet_dir()?;
    let loaded_wallets = rpc.list_wallets()?;
    if !available_wallets.contains(&wallet_name.to_string()) {
        // create wallet
        rpc.create_wallet(wallet_name, None, None, None, None)?;
    } else if !loaded_wallets.contains(&wallet_name.to_owned()) {
        rpc.load_wallet(wallet_name)?;
    }

    let auth = Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned());
    let wallet_rpc_url = format!("{}/wallet/{}", RPC_URL, wallet_name);
    Client::new(&wallet_rpc_url, auth.clone())
}

// prepares Miner and Trader wallets
// returns tuple: (miner_wallet_rpc: Client, trader_wallet_rpc: Client)
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
            // generate SegWit address
            Some(bitcoincore_rpc::json::AddressType::Bech32),
        )?
        // we could assume network is checked as it was generated
        // but let's make sure by requiring regtest
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        // manually map error
        .map_err(|e| bitcoincore_rpc::Error::ReturnedError(e.to_string()))?;

    // generate to address with 101 blocks
    // need 100 after first coinbase for it to become spendable
    miner_rpc.generate_to_address(101, &miner_address)?;

    let trader_address = trader_rpc
        .get_new_address(
            Some("first trader receive"),
            // generate SegWit address
            Some(bitcoincore_rpc::json::AddressType::Bech32),
        )?
        // same as before: let's make sure by requiring regtest
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        // manually map error
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
    // calling get_mempool_entry will fail if txid is not in there
    rpc.get_mempool_entry(&txid).map_err(|e| {
        // print details here to make it more obvious
        println!("Cannot find txid {} in mempool: {}", txid, e);

        // then return initial error
        e
    })?;

    // Mine 1 block to confirm the transaction
    miner_rpc.generate_to_address(1, &miner_address)?;

    // Extract all required transaction details
    let tx_info = rpc.get_raw_transaction_info(&txid, None)?;
    let tx: Transaction = tx_info.transaction()?;

    // get block hash
    let block_hash = tx_info
        .blockhash
        .ok_or(ReturnedError(String::from("block not confirmed")))?;

    // get block info that contains height for a given hash
    let block_info = rpc.get_block_info(&block_hash)?;

    let mut addr_in = String::from("not initialized");
    let mut amt_in: f64 = 0.0;
    let mut addr_out = String::from("not initialized");
    let mut amt_out: f64 = 0.0;
    let mut addr_change = String::from("not initialized");
    let mut amt_change: f64 = 0.0;

    // Process every input to calculate amount in
    for input in &tx.input {
        if let Ok(prev_tx) = rpc.get_raw_transaction(&input.previous_output.txid, None) {
            let prev_out = &prev_tx.output[input.previous_output.vout as usize];
            amt_in += prev_out.value.to_btc();

            // a bit awkward: the test only wants one address, so overwrite the address.
            // this (1 input address) only really makes sense on a fresh regtest where the spend
            // will be from a coinbase only. Test will still pass after it constructs multiple
            // inputs, but the single input address doesn't make sense then.
            let address = Address::from_script(
                &prev_out.script_pubkey,
                bitcoincore_rpc::bitcoin::Network::Regtest,
            )
            .map_err(|e| ReturnedError(e.to_string()))?;
            addr_in = address.to_string();
        }
    }

    // address/amount out and change are in the out vector
    // if not 2 outputs, something is wrong, return error
    if tx.output.len() != 2 {
        return Err(ReturnedError(String::from("unexpected output count")));
    }

    for output in &tx.output {
        let addr = Address::from_script(
            &output.script_pubkey,
            bitcoincore_rpc::bitcoin::Network::Regtest,
        )
        .map_err(|e| ReturnedError(e.to_string()))?;

        if addr == trader_address {
            addr_out = addr.to_string();
            amt_out = output.value.to_btc();
        } else {
            addr_change = addr.to_string();
            amt_change = output.value.to_btc();
        }
    }

    let fees = amt_in - amt_out - amt_change;

    // prepare simplistic vector for output, to be reworked
    let mut lines: Vec<String> = vec![
        // transaction id
        tx.txid().to_string(),
        // miner's input address (well, one of them anyway)
        addr_in,
        // miner's input amount (in btc, 8 digits)
        format!("{:.8}", amt_in),
        // trader's output address
        addr_out,
        // trader's output amount (in btc)
        format!("{:.8}", amt_out),
        // miner's change address
        addr_change,
        // miner's change amount (in btc)
        format!("{:.8}", amt_change),
        // fees (in btc)
        format!("{:.8}", fees),
        // block height
        block_info.height.to_string(),
        // block hash
        block_hash.to_string(),
    ];

    // Write the data to ../out.txt in the specified format given in readme.md
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("../out.txt")?;

    let mut writer = BufWriter::new(file);

    for line in lines {
        writeln!(writer, "{}", line)?;
    }

    writer.flush();

    Ok(())
}
