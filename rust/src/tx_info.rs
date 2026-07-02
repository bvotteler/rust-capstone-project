use bitcoin::{Address, Amount, BlockHash, Txid};
use std::fmt;

// Combine address and amount as they're grouped for input, output, and change
pub struct AddressAmount {
    pub address: Address,
    pub amount: Amount,
}

// Struct to hold test tx data
pub struct TxInfo {
    // Transaction id
    pub txid: Txid,
    // Miner's input address & amount
    pub input: AddressAmount,
    // Trader's output address & amount
    pub output: AddressAmount,
    // Miner's change address & amount
    pub change: AddressAmount,
    // Block height
    pub block_height: u32,
    // Block hash
    pub block_hash: BlockHash,
}

impl TxInfo {
    pub fn builder() -> TxInfoBuilder {
        TxInfoBuilder::new()
    }
}

// Implement Display to write expected format to file.
// We can lean on Display traits implemented for most bitcoincore_rpc types.
impl fmt::Display for TxInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.txid)?;
        // Miner input
        writeln!(f, "{}", self.input.address)?;
        writeln!(f, "{:.8}", self.input.amount.to_btc())?;
        // Trader output
        writeln!(f, "{}", self.output.address)?;
        writeln!(f, "{:.8}", self.output.amount.to_btc())?;
        // Miner change
        writeln!(f, "{}", self.change.address)?;
        writeln!(f, "{:.8}", self.change.amount.to_btc())?;

        // Use Amount calcs then convert to btc to avoid f64 precision issues
        let fees = self.input.amount - self.output.amount - self.change.amount;
        writeln!(f, "{:.8}", fees.to_btc())?;

        writeln!(f, "{}", self.block_height)?;
        // Note: no new line after last item
        write!(f, "{}", self.block_hash)
    }
}

// Builder for TxInfo
pub struct TxInfoBuilder {
    txid: Option<Txid>,
    input: Option<AddressAmount>,
    output: Option<AddressAmount>,
    change: Option<AddressAmount>,
    block_height: Option<u32>,
    block_hash: Option<BlockHash>,
}

impl TxInfoBuilder {
    pub fn new() -> Self {
        Self {
            txid: None,
            input: None,
            output: None,
            change: None,
            block_height: None,
            block_hash: None,
        }
    }

    pub fn txid(mut self, txid: Txid) -> Self {
        self.txid = Some(txid);
        self
    }

    pub fn input(mut self, input: AddressAmount) -> Self {
        self.input = Some(input);
        self
    }

    pub fn output(mut self, output: AddressAmount) -> Self {
        self.output = Some(output);
        self
    }

    pub fn change(mut self, change: AddressAmount) -> Self {
        self.change = Some(change);
        self
    }

    pub fn block_height(mut self, block_height: u32) -> Self {
        self.block_height = Some(block_height);
        self
    }

    pub fn block_hash(mut self, block_hash: BlockHash) -> Self {
        self.block_hash = Some(block_hash);
        self
    }

    pub fn build(self) -> Result<TxInfo, &'static str> {
        Ok(TxInfo {
            txid: self.txid.ok_or("Missing txid")?,
            input: self.input.ok_or("Missing input")?,
            output: self.output.ok_or("Missing output")?,
            change: self.change.ok_or("Missing change")?,
            block_height: self.block_height.ok_or("Missing block_height")?,
            block_hash: self.block_hash.ok_or("Missing block_hash")?,
        })
    }
}

impl Default for TxInfoBuilder {
    fn default() -> Self {
        Self::new()
    }
}
