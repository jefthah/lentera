//! Pembaca tabel Paradox, format database EasyWorship 2009.
//!
//! Hanya membaca, dan hanya tipe kolom yang dipakai tabel lagu: teks
//! (Alpha), bilangan bulat, logika, dan memo. Memo yang panjang disimpan di
//! berkas `.MB` di sebelah berkas `.DB`. Kolom lain (gambar, tanggal, dan
//! sebagainya) dibaca sebagai [`Value::Other`].

use super::cp1252_string;

/// Nilai satu kolom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    Null,
    Text(String),
    Int(i64),
    Bool(bool),
    Other,
}

impl Value {
    pub fn text(&self) -> &str {
        match self {
            Value::Text(text) => text,
            _ => "",
        }
    }
}

#[derive(Debug)]
pub(super) struct Table {
    pub names: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n.eq_ignore_ascii_case(name))
    }
}

const NOT_PARADOX: &str = "Berkas ini bukan tabel Paradox EasyWorship 2009.";

fn bytes(data: &[u8], offset: usize, len: usize) -> Result<&[u8], String> {
    data.get(offset..offset.saturating_add(len))
        .ok_or_else(|| "Tabel Paradox rusak: data terpotong.".to_owned())
}

fn u16_at(data: &[u8], offset: usize) -> Result<usize, String> {
    let b = bytes(data, offset, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]) as usize)
}

/// Bilangan Paradox: big-endian dengan bit tanda dibalik; semua nol berarti
/// kosong.
fn paradox_int(b: &[u8]) -> Option<i64> {
    if b.iter().all(|&x| x == 0) {
        return None;
    }
    let mut value: u64 = (b[0] ^ 0x80) as u64;
    for &x in &b[1..] {
        value = (value << 8) | x as u64;
    }
    let bits = b.len() as u32 * 8;
    // Perluas tanda dari lebar kolom ke 64 bit.
    let shift = 64 - bits;
    Some(((value << shift) as i64) >> shift)
}

/// Membaca seluruh baris tabel. `memo` berisi berkas `.MB`, bila ada.
pub(super) fn read(db: &[u8], memo: Option<&[u8]>) -> Result<Table, String> {
    if db.len() < 0x78 {
        return Err(NOT_PARADOX.into());
    }
    let record_size = u16_at(db, 0)?;
    let header_size = u16_at(db, 2)?;
    let file_type = db[4];
    let block_size = db[5] as usize * 1024;
    let version = db[0x39];
    let fields = u16_at(db, 0x21)?;
    if record_size == 0
        || header_size < 0x78
        || !matches!(file_type, 0 | 2)
        || !(1..=32).contains(&db[5])
        || !(3..=12).contains(&version)
        || fields == 0
        || fields > 255
    {
        return Err(NOT_PARADOX.into());
    }

    let descriptors = if version >= 5 { 0x78 } else { 0x58 };
    let mut types = Vec::with_capacity(fields);
    let mut sizes = Vec::with_capacity(fields);
    for i in 0..fields {
        let b = bytes(db, descriptors + i * 2, 2)?;
        types.push(b[0]);
        sizes.push(b[1] as usize);
    }
    if sizes.iter().sum::<usize>() != record_size {
        return Err(NOT_PARADOX.into());
    }
    // Setelah deskriptor: penunjuk nama tabel, penunjuk nama kolom, lalu nama
    // tabel dengan panjang tetap, lalu nama kolom yang diakhiri nol.
    let mut p = descriptors + fields * 2 + 4 + fields * 4;
    p += if version >= 12 { 261 } else { 79 };
    let mut names = Vec::with_capacity(fields);
    for _ in 0..fields {
        let rest = db.get(p..).ok_or(NOT_PARADOX)?;
        let end = rest.iter().position(|&b| b == 0).ok_or(NOT_PARADOX)?;
        names.push(cp1252_string(&rest[..end]));
        p += end + 1;
    }

    let mut rows = Vec::new();
    let mut block = u16_at(db, 0x0E)?;
    let mut seen = std::collections::HashSet::new();
    while block != 0 && seen.insert(block) {
        let start = header_size + (block - 1) * block_size;
        let next = u16_at(db, start)?;
        let added = i16::from_le_bytes(bytes(db, start + 4, 2)?.try_into().unwrap_or([0; 2]));
        if added >= 0 {
            let count = added as usize / record_size + 1;
            for r in 0..count {
                let record = bytes(db, start + 6 + r * record_size, record_size)?;
                let mut offset = 0;
                let mut row = Vec::with_capacity(fields);
                for (&kind, &size) in types.iter().zip(&sizes) {
                    row.push(value(kind, &record[offset..offset + size], memo));
                    offset += size;
                }
                rows.push(row);
            }
        }
        block = next;
    }
    Ok(Table { names, rows })
}

fn value(kind: u8, b: &[u8], memo: Option<&[u8]>) -> Value {
    match kind {
        // Alpha: teks Windows-1252 yang diakhiri nol.
        0x01 => {
            let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
            if end == 0 {
                Value::Null
            } else {
                Value::Text(cp1252_string(&b[..end]))
            }
        }
        // Short, Long, Autoincrement.
        0x03 | 0x04 | 0x16 => paradox_int(b).map_or(Value::Null, Value::Int),
        0x09 => match b.first() {
            Some(0) | None => Value::Null,
            Some(&x) => Value::Bool(x == 0x81),
        },
        // Memo dan Formatted Memo.
        0x0C | 0x0E => match blob(b, memo) {
            Some(data) => {
                let end = data.iter().rposition(|&x| x != 0).map_or(0, |i| i + 1);
                Value::Text(cp1252_string(&data[..end]))
            }
            None => Value::Null,
        },
        _ => Value::Other,
    }
}

/// Isi memo: pendek di dalam baris, panjang di berkas `.MB`.
fn blob<'a>(field: &'a [u8], memo: Option<&'a [u8]>) -> Option<&'a [u8]> {
    if field.len() < 10 {
        return None;
    }
    let leader = field.len() - 10;
    let offset = u32::from_le_bytes(field[leader..leader + 4].try_into().ok()?) as usize;
    let length = i32::from_le_bytes(field[leader + 4..leader + 8].try_into().ok()?);
    if length <= 0 {
        return None;
    }
    let length = length as usize;
    if length <= leader {
        return field.get(..length);
    }
    let mb = memo?;
    let index = offset & 0xFF;
    let block = offset & !0xFF;
    let start = if index == 0xFF {
        // Satu blob dalam satu blok (tipe 2).
        (*mb.get(block)? == 2).then_some(block + 9)?
    } else {
        // Beberapa blob kecil dalam satu blok (tipe 3).
        if *mb.get(block)? != 3 {
            return None;
        }
        block + *mb.get(block + 12 + index * 5)? as usize * 16
    };
    mb.get(start..start.checked_add(length)?)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Kolom tabel uji: nama, tipe, ukuran.
    pub type Field<'a> = (&'a str, u8, u8);

    /// Membuat tabel Paradox 7 kecil (satu blok data) untuk pengujian.
    pub fn table(fields: &[Field], records: &[Vec<u8>]) -> Vec<u8> {
        let record_size: usize = fields.iter().map(|f| f.2 as usize).sum();
        let header_size = 0x800;
        let mut db = vec![0u8; header_size];
        db[0..2].copy_from_slice(&(record_size as u16).to_le_bytes());
        db[2..4].copy_from_slice(&(header_size as u16).to_le_bytes());
        db[4] = 0;
        db[5] = 1; // blok 1 KB
        db[6..10].copy_from_slice(&(records.len() as i32).to_le_bytes());
        db[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes());
        db[0x21..0x23].copy_from_slice(&(fields.len() as u16).to_le_bytes());
        db[0x39] = 12;
        let mut p = 0x78;
        for f in fields {
            db[p] = f.1;
            db[p + 1] = f.2;
            p += 2;
        }
        p += 4 + fields.len() * 4 + 261;
        for f in fields {
            db[p..p + f.0.len()].copy_from_slice(f.0.as_bytes());
            p += f.0.len() + 1;
        }
        let mut block = vec![0u8; 1024];
        let added = (records.len() as i16 - 1) * record_size as i16;
        block[4..6].copy_from_slice(&added.to_le_bytes());
        for (i, record) in records.iter().enumerate() {
            block[6 + i * record_size..6 + (i + 1) * record_size].copy_from_slice(record);
        }
        db.extend(block);
        db
    }

    /// Kolom Alpha; `text` sudah dalam Windows-1252.
    pub fn alpha(text: &[u8], size: usize) -> Vec<u8> {
        let mut b = text.to_vec();
        b.resize(size, 0);
        b
    }

    pub fn long(value: i32) -> Vec<u8> {
        let mut b = value.to_be_bytes();
        b[0] ^= 0x80;
        b.to_vec()
    }

    /// Kolom memo yang isinya di blok `.MB` tipe 2 pada `block`.
    pub fn memo_field(size: usize, block: u32, length: usize) -> Vec<u8> {
        let mut b = vec![0u8; size];
        let leader = size - 10;
        b[leader..leader + 4].copy_from_slice(&(block | 0xFF).to_le_bytes());
        b[leader + 4..leader + 8].copy_from_slice(&(length as i32).to_le_bytes());
        b
    }

    #[test]
    fn reads_text_numbers_and_memos() {
        let long_text = "x".repeat(40);
        let mut mb = vec![0u8; 0x100];
        mb.push(2);
        mb.extend([0u8; 8]);
        mb.extend(long_text.as_bytes());

        // Memo pendek langsung di dalam baris.
        let mut short_memo = b"pendek".to_vec();
        short_memo.resize(20 - 10, 0);
        short_memo.extend(0u32.to_le_bytes());
        short_memo.extend(6i32.to_le_bytes());
        short_memo.extend([0u8; 2]);

        let fields = [("Title", 0x01, 10), ("RecID", 0x04, 4), ("Words", 0x0C, 20)];
        let records = vec![
            [alpha(b"Caf\xe9 \x93A\x94", 10), long(7), short_memo].concat(),
            [alpha(b"", 10), long(-3), memo_field(20, 0x100, 40)].concat(),
        ];
        let t = read(&table(&fields, &records), Some(&mb)).unwrap();
        assert_eq!(t.names, ["Title", "RecID", "Words"]);
        assert_eq!(t.column("words"), Some(2));
        assert_eq!(t.rows.len(), 2);
        assert_eq!(
            t.rows[0][0],
            Value::Text("Caf\u{e9} \u{201c}A\u{201d}".into())
        );
        assert_eq!(t.rows[0][1], Value::Int(7));
        assert_eq!(t.rows[0][2], Value::Text("pendek".into()));
        assert_eq!(t.rows[1][0], Value::Null);
        assert_eq!(t.rows[1][1], Value::Int(-3));
        assert_eq!(t.rows[1][2].text(), long_text);

        // Tanpa berkas .MB, memo panjang kosong.
        let t = read(&table(&fields, &records), None).unwrap();
        assert_eq!(t.rows[1][2], Value::Null);
    }

    #[test]
    fn rejects_other_files() {
        assert!(read(b"bukan paradox", None).is_err());
        assert!(read(&[0u8; 4096], None).is_err());
        let fields = [("Title", 0x01, 10)];
        let mut db = table(&fields, &[alpha(b"A", 10)]);
        db.truncate(0x800 + 8);
        assert!(read(&db, None).is_err());
    }
}
