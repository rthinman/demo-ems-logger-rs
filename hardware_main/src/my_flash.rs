use embedded_nand_async::NandFlash;
use spi_nand_devices::winbond::w25n::asyn;
use spi_nand::cmd_async::SpiNandAsync;
use spi_nand::{ECCStatus, SpiNand, SpiNandDevice};
use embedded_hal_async::spi::SpiDevice;
use spi_nand_devices::winbond::w25n::{asyn::{BBMAsync, ECCBasicAsync, ODSAsync}, W25N01GW};

use crate::dhara_nand_async::{DharaNandAsync, DharaPage, DharaBlock, DharaError};

pub struct MyFlash<SPI, D, const N: usize> 
where 
    SPI: SpiDevice,
    D: SpiNandAsync<SPI, N> + ECCBasicAsync<SPI, N> + core::fmt::Debug,
{
    nand: SpiNandDevice<SPI, D, N>,
    log2_page_size: u8,
    log2_ppb: u8,
    num_blocks: u32,
    layout_buffer: [u8; 2050], // TODO: make configurable.
}

impl<SPI, D, const N: usize> MyFlash<SPI, D, N> 
where 
    SPI: SpiDevice,
    D: SpiNandAsync<SPI, N> + ECCBasicAsync<SPI, N> + core::fmt::Debug,
{
    pub fn new(nand: SpiNandDevice<SPI, D, N>, log2_page_size: u8, log2_ppb: u8, num_blocks: u32) -> Self {
        let layout_buffer = [0u8; 2050]; // TODO: make configurable.
        Self {
            nand,
            log2_page_size,
            log2_ppb,
            num_blocks,
            layout_buffer,
        }
    }

    pub async fn initialize(&mut self) -> Result<(), <SpiNandDevice<SPI, D, N> as embedded_nand_async::ErrorType>::Error> {
        // Wait until the device is ready.
        while self.nand.device.is_busy(&mut self.nand.spi).await? {}
        // Reset the device.
        self.nand.device.reset_cmd(&mut self.nand.spi).await?;
        // self.nand.device.reset_async(&mut self.nand.spi).await?;
        // wait until ready again.
        while self.nand.device.is_busy(&mut self.nand.spi).await? {}
        // Check that the correct IC is installed.
        // self.nand.device.verify_id_async(&mut self.nand.spi).await?;
        // By default, the W25N01GW powers up with block protection enabled.
        self.nand.device.disable_block_protection(&mut self.nand.spi).await
    }
}


impl<SPI, D, const N: usize> DharaNandAsync<SpiNandDevice<SPI, D, N>> for MyFlash<SPI, D, N> 
where 
    SPI: SpiDevice,
    D: SpiNandAsync<SPI, N> + ECCBasicAsync<SPI, N> + core::fmt::Debug,
{
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
            Ok(_) => true, // Any other status, consider it bad
            Err(_) => true, // If read fails, consider it bad.
        }
    }

    async fn mark_bad(&mut self, blk: DharaBlock) -> () {
        let block = embedded_nand::BlockIndex::new(blk as u16);
        let _ = self.nand.mark_block_bad(block).await;
        // Ignore result, as there's nothing we can do if it fails.
    }

    async fn erase(&mut self, blk: DharaBlock) -> Result<(),DharaError<SpiNandDevice<SPI, D, N>>> {
        let block = embedded_nand::BlockIndex::new(blk as u16);
        self.nand.erase_block(block).await.map_err(|e| DharaError::Flash(e))
    }

    async fn prog(&mut self, page: DharaPage, data: &[u8]) -> Result<(),DharaError<SpiNandDevice<SPI, D, N>>> {
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

    async fn read(&mut self, page: u32, offset: usize, length: usize, data: &mut[u8]) -> Result<(), DharaError<SpiNandDevice<SPI, D, N>>> {
        let page_index = embedded_nand::PageIndex::new(page);
        let column_addr = embedded_nand::ColumnAddress::new(offset as u16);
        
        // Perform the read operation
        match self.nand.read_page_slice_async(page_index, column_addr, &mut data[..length]).await {
            Ok(_) => {
                // Check ECC status after successful read
                match self.nand.device.ecc_status(&mut self.nand.spi).await {
                    Ok(spi_nand::ECCStatus::Ok) => {
                        // No ECC errors detected
                        Ok(())
                    },
                    Ok(spi_nand::ECCStatus::Corrected) => {
                        // ECC corrected errors - log warning but return success
                        defmt::warn!("ECC corrected errors on page {}", page);
                        Ok(())
                    },
                    Ok(spi_nand::ECCStatus::Failed) => {
                        // Uncorrectable ECC errors
                        defmt::error!("Uncorrectable ECC errors on page {}", page);
                        Err(DharaError::ECC)
                    },
                    Ok(spi_nand::ECCStatus::Failing) => {
                        // ECC corrected but approaching threshold
                        defmt::warn!("ECC failing threshold approached on page {}", page);
                        Ok(())
                    },
                    Err(e) => {
                        // Error reading ECC status
                        defmt::error!("Failed to read ECC status for page {}", page);
                        Err(DharaError::Flash(e.into()))
                    }
                }
            },
            Err(e) => {
                // Flash read operation failed
                Err(DharaError::Flash(e))
            }
        }
        
        // TODO: cache data and/or metadata.
    }

    async fn copy(&mut self, src: DharaPage, dst: DharaPage) -> Result<(),DharaError<SpiNandDevice<SPI, D, N>>> {
        let src_page_index = embedded_nand::PageIndex::new(src);
        let dst_page_index = embedded_nand::PageIndex::new(dst);
        
        // Step 1: Load source page into device's internal buffer
        self.nand.device.page_read_cmd(&mut self.nand.spi, src_page_index).await.map_err(|e| DharaError::Flash(e))?;
        
        // Wait for read to complete
        while self.nand.device.is_busy(&mut self.nand.spi).await.map_err(|e| DharaError::Flash(e))? {}
        
        // Step 2: Check ECC status after reading into internal buffer
        match self.nand.device.ecc_status(&mut self.nand.spi).await {
            Ok(spi_nand::ECCStatus::Ok) => {
                // Source read OK, proceed with copy
            },
            Ok(spi_nand::ECCStatus::Corrected) => {
                // ECC corrected errors on source - log but continue
                defmt::warn!("ECC corrected errors when copying from page {}", src);
            },
            Ok(spi_nand::ECCStatus::Failed) => {
                // Uncorrectable ECC errors on source
                defmt::error!("Uncorrectable ECC errors when copying from page {}", src);
                return Err(DharaError::ECC);
            },
            Ok(spi_nand::ECCStatus::Failing) => {
                // ECC approaching threshold - log but continue
                defmt::warn!("ECC failing threshold when copying from page {}", src);
            },
            Err(e) => {
                defmt::error!("Failed to read ECC status when copying from page {}", src);
                return Err(DharaError::Flash(e.into()));
            }
        }
        
        // Step 3: Enable writing
        self.nand.device.write_enable_cmd(&mut self.nand.spi).await.map_err(|e| DharaError::Flash(e))?;
        
        // Step 4: Program from internal buffer to destination
        self.nand.device.program_execute_cmd(&mut self.nand.spi, dst_page_index).await.map_err(|e| DharaError::Flash(e))?;
        
        // Step 5: Wait for program to complete
        while self.nand.device.is_busy(&mut self.nand.spi).await.map_err(|e| DharaError::Flash(e))? {}
        
        // Step 6: Check if program operation succeeded
        if self.nand.device.program_failed(&mut self.nand.spi).await.map_err(|e| DharaError::Flash(e))? {
            defmt::error!("Program failed when copying to page {}", dst);
            return Err(DharaError::Flash(spi_nand::error::SpiFlashError::ProgramFailed.into()));
        }
        
        Ok(())
    }
}

/// Compute the base-2 logarithm of an integer, rounded down.
/// Panics if x is 0.
pub fn log2(x: u32) -> u8 {
    31 - x.leading_zeros() as u8
}
