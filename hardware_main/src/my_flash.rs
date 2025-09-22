use embedded_nand_async::NandFlash;
use spi_nand_devices::winbond::w25n::asyn;
use spi_nand::cmd_async::SpiNandAsync;
use spi_nand::{ECCStatus, SpiNand, SpiNandDevice};
//use spi_nand_devices::winbond::w25n::{asyn::{BBMAsync, ECCBasicAsync, ODSAsync}, W25N01GW};

use crate::dhara_nand_async::{DharaNandAsync, DharaPage, DharaBlock, DharaError};

pub struct MyFlash<NF: NandFlash + SpiNandAsync<SPI, {N}>> {
    nand: NF,
    log2_page_size: u8,
    log2_ppb: u8,
    num_blocks: u32,
    layout_buffer: [u8; 2050], // TODO: make configurable.
}

impl<NF: NandFlash + SpiNandAsync<SPI, {N}>> MyFlash<NF> {
    pub fn new(nand: NF, log2_page_size: u8, log2_ppb: u8, num_blocks: u32) -> Self {
        let layout_buffer = [0u8; 2050]; // TODO: make configurable.
        Self {
            nand,
            log2_page_size,
            log2_ppb,
            num_blocks,
            layout_buffer,
        }
    }

    pub async fn initialize(&mut self) -> Result<(), NF::Error> {
        // Wait until the device is ready.
        while self.nand.device.is_busy(&mut self.nand.spi).await? {}
        // Reset the device.
        self.nand.reset_async().await?;
        // wait until ready again.
        while self.nand.device.is_busy(&mut self.nand.spi).await? {}
        // Check that the correct IC is installed.
        self.nand.verify_id_async().await?;
        // By default, the W25N01GW powers up with block protection enabled.
        self.nand.device.disable_block_protection(&mut self.nand.spi).await
    }
}


impl<NF: NandFlash + SpiNandAsync<SPI, {N}>> DharaNandAsync<F> for MyFlash<NF> {
    fn get_log2_page_size(&self) -> u8 {
        self.log2_page_size
    }

    fn get_log2_ppb(&self) -> u8 {
        self.log2_ppb
    }

    fn get_num_blocks(&self) -> u32 {
        self.num_blocks
    }

    // TODO: change to return Result<bool, DharaError>?
    async fn is_bad(&mut self, blk: DharaBlock) -> bool {
        let block = embedded_nand::BlockIndex::new(blk as u16);
        match self.nand.block_status(block).await {
            Ok(embedded_nand::BlockStatus::Ok) => false,
            Ok(embedded_nand::BlockStatus::Failed) => true,
            Err(_) => true, // If read fails, consider it bad.
        }
    }

    async fn mark_bad(&mut self, blk: DharaBlock) -> () {
        let block = embedded_nand::BlockIndex::new(blk as u16);
        let _ = self.nand.mark_block_bad(block).await;
        // Ignore result, as there's nothing we can do if it fails.
    }

    async fn erase(&mut self, blk: DharaBlock) -> Result<(),DharaError<F>> {
        let block = embedded_nand::BlockIndex::new(blk as u16);
        self.nand.erase_block(block).await.map_err(|e| DharaError::Flash(e))
    }

    async fn prog(&mut self, page: DharaPage, data: &[u8]) -> Result<(),DharaError<F>> {
        let page_index = embedded_nand::PageIndex::new(page);
        let column_addr = embedded_nand::ColumnAddress::new(0);
        self.layout_buffer[..data.len()].copy_from_slice(data);
        // Add seal byte to the end of the data.
        self.layout_buffer[data.len()] = 0xFF; // Good block marker.
        self.layout_buffer[data.len() + 1] = 0x00; // The seal is in the second spare byte.
        self.nand.write_page_slice_async(page_index, column_addr, &self.layout_buffer).await.map_err(|e| DharaError::Flash(e))
    }

    async fn is_free(&mut self, page: DharaPage) -> bool {
        let page_index = embedded_nand::PageIndex::new(page);
        let column_addr = embedded_nand::ColumnAddress::new(2048); // Start of spare area. TODO: make configurable.
        let mut buf = [0u8; 2]; // Bad block marker + seal byte.
        match self.nand.read_page_slice_async(page_index, column_addr, &mut buf).await {
            Ok(_) => {
                // Seal byte in the erased state means that the page is free.
                buf[1] == 0xFF
            },
            Err(_) => false, // If read fails, consider it already programmed. TODO: is this the right choice?
        }
    }

    async fn read(&mut self, page: u32, offset: usize, length: usize, data: &mut[u8]) -> Result<(), DharaError<F>> {
        let page_index = embedded_nand::PageIndex::new(page);
        let column_addr = embedded_nand::ColumnAddress::new(offset as u16);
        self.nand.read_page_slice_async(page_index, column_addr, &mut data[..length]).await.map_err(|e| DharaError::Flash(e))
        // TODO: implement ECC.
        // TODO: cache data and/or metadata.
    }

    async fn copy(&mut self, src: DharaPage, dst: DharaPage) -> Result<(),DharaError<F>> {
        let src_page_index = embedded_nand::PageIndex::new(src);
        let dst_page_index = embedded_nand::PageIndex::new(dst);
        // The W25N01GW copies the full page plus spare area internally.
        // TODO: this is the simple way, but does not handle ECC.
        // For that, we need to read first, reaching deeper into the implementation, check ECC, then write.
        self.nand.copy_page_async(src_page_index, dst_page_index).await.map_err(|e| DharaError::Flash(e))
    }
}

/// Compute the base-2 logarithm of an integer, rounded down.
/// Panics if x is 0.
pub fn log2(x: u32) -> u8 {
    31 - x.leading_zeros() as u8
}
