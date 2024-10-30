use std::{error::Error, fs::File, io::{self, BufReader, Read, Seek, SeekFrom}, str};
use bzip2::read::BzDecoder;
use xz2::read::XzDecoder;

use crate::chromeos_update_engine::{install_operation::Type, DeltaArchiveManifest, PartitionUpdate};

const PAYLOAD_HEADER_MAGIC: &str = "CrAU";
const BRILLO_MAJOR_PAYLOAD_VERSION: u64 = 2;
const BLOCK_SIZE: u64 = 4096;

pub struct Payload {
    path: String,
    file: File,
    header: Option<PayloadHeader>,
    manifest: Option<DeltaArchiveManifest>,
}

pub struct PayloadHeader {
    version: u64,
    size: u64,
    manifest_len: u64,
    signature_len: u32,
    data_offset: u64,
    metadata_size: u64
}

impl Payload {
    pub fn new(path: String) -> Result<Payload, Box<dyn Error>> {
        let file = match File::open(path.clone()) {
            Ok(f) => f,
            Err(err) => {
                return Err(format!("Err:{}", err).into());
            }
        };
        Ok(Payload {
            path,
            file,
            header: None,
            manifest: None,
        })
    }

    fn init(&mut self) -> Result<(), Box<dyn Error>> {
        match self.read_header() {
            Ok(header) => self.header = Some(header),
            Err(err) => {
                return Err(err);
            }
        }

        match self.read_manifest() {
            Ok(manifest) => self.manifest = Some(manifest),
            Err(err) => {
                return Err(err);
            }
        }
        Ok(())
    }

    fn read_header(&mut self) -> Result<PayloadHeader, Box<dyn Error>> {
        let mut buf = [0; 4];

        self.file.read_exact(&mut buf)?;

        if str::from_utf8(&buf)? != PAYLOAD_HEADER_MAGIC {
            return Err("Invalid Payload magic".into());
        }
        let mut header = PayloadHeader {
            version: 0,
            manifest_len: 0,
            signature_len: 0,
            size: 0,
            data_offset: 0,
            metadata_size: 0
        };

        let mut buf = [0; 8];
        self.file.read_exact(&mut buf)?;
        header.version = u64::from_be_bytes(buf);

        if header.version != BRILLO_MAJOR_PAYLOAD_VERSION {
            return Err("Unsupported payload version".into());
        }

        let mut buf = [0; 8];
        self.file.read_exact(&mut buf)?;
        header.manifest_len = u64::from_be_bytes(buf);

        let mut buf = [0; 4];
        self.file.read_exact(&mut buf)?;
        header.signature_len = u32::from_be_bytes(buf);

        header.size = 24;
        header.metadata_size = header.size + header.manifest_len;
        header.data_offset = header.signature_len as u64 + header.metadata_size;

        Ok(header)
    }

    fn read_manifest(&mut self) -> Result<DeltaArchiveManifest, Box<dyn Error>> {
        let manifest_len = self.header.as_ref().unwrap().manifest_len as usize;
        let mut manifest_buf = vec![0; manifest_len];

        self.file.read_exact(&mut manifest_buf)?;

        let delta_manifest: DeltaArchiveManifest = prost::Message::decode(&manifest_buf[..])?;

        Ok(delta_manifest)
    }

    pub fn extract(&mut self, partition_to_extract: &str, out_file: &str) -> Result<String, Box<dyn Error>> {
        if let Err(err) = self.init() {
            return Err(err);
        }

        if let Some(manifest) = &self.manifest {
            let mut partition: Option<&PartitionUpdate> = None;
            let partitions = manifest.partitions.clone();
            for (_, p) in partitions.iter().enumerate() {
                if partition_to_extract == p.partition_name {
                    partition = Some(p);
                    if let Err(err) = self.extract_selected(p, out_file) {
                        return Err(err);
                    }
                };
            }
            if partition.is_none() {
                return Err(format!("partition: {} not found in {}", partition_to_extract, &self.path).into());
            }
        }

        Ok("Done".into())
    }

    fn extract_selected(&mut self, partition: &PartitionUpdate, out_file: &str) -> Result<(), Box<dyn Error>> {
        let mut output_file = match File::create(out_file) {
            Ok(f) => {
                f
            }
            Err(err) => {
                return Err(format!("file create error: {}", err).into());
            }
        };
        let name = &partition.partition_name;
        // let info = partition.new_partition_info.as_ref().unwrap();
        // let total_operations = partition.operations.len() as u64;
        // let size = info.size.as_ref().unwrap();

        let mut reader = BufReader::new(&self.file);

        for operation in &partition.operations {
            if operation.dst_extents.is_empty() {
                return Err(format!("invalid dstextents for partition: {}", name).into());
            }

            let dst = operation.dst_extents[0];
            let data_offset = operation.data_offset.unwrap() + self.header.as_ref().unwrap().data_offset;
            let data_length = operation.data_length.unwrap();
            let expected_uncompress_block_size = dst.num_blocks() * BLOCK_SIZE;

            let _ = reader.seek(SeekFrom::Start(data_offset));
            let mut reader = Read::take(&mut reader, data_length);

            match operation.r#type() {
                Type::Replace => {
                    let _ = io::copy(&mut reader, &mut output_file);
                },
                Type::ReplaceXz => {
                    let mut decoder = XzDecoder::new(reader);
                    let _ = io::copy(&mut decoder, &mut output_file);
                },
                Type::ReplaceBz => {
                    let mut decoder = BzDecoder::new(reader);
                    let _ = io::copy(&mut decoder, &mut output_file);
                },
                Type::Zero => {
                    let mut filler = io::repeat(0).take(expected_uncompress_block_size);
                    let n = io::copy(&mut filler, &mut output_file)?;
                    if n != expected_uncompress_block_size {
                        return Err(format!("Err:writing zero: {}", name).into())
                    }
                },
                _ => {
                    return Err(format!("Unsupported operation type: {}", operation.r#type).into());
                }
            }
        }

        Ok(())
    }

    pub fn get_partition_list(&mut self) -> Result<String, Box<dyn Error>> {

        let mut msg: String = Default::default();

        if let Err(err) = self.init() {
            return Err(err);
        }
        
        if let Some(manifest) = &self.manifest {
           
            msg.insert_str(msg.len(), "Partitions:");

            for (i, partition) in manifest.partitions.iter().enumerate() {
                let partition_name = &partition.partition_name;
                let partition_size = partition.new_partition_info.as_ref().map_or(0, |info| info.size.expect("info size not found"));

                let mg = format!("{}|{},", partition_name, partition_size);
                msg.insert_str(msg.len(), mg.as_str());

                if i < manifest.partitions.len() - 1 {
                    print!(", ");
                } else {
                    println!();
                }
            }
        } else {
            msg = String::from("No partitions found");
        }
        Ok(msg)
    }
}