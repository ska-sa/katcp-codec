/* Copyright (c) 2024, National Research Foundation (SARAO)
 *
 * Licensed under the BSD 3-Clause License (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy
 * of the License at
 *
 *   https://opensource.org/licenses/BSD-3-Clause
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;

fn escape(c: u8) -> u8 {
    match c {
        b'\r' => b'r',
        b'\n' => b'n',
        b'\t' => b't',
        b'\x1B' => b'e',
        b'\0' => b'0',
        b'\\' => b'\\',
        b' ' => b'_',
        _ => 0, // Marker for not needing an escape
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);
    let tables_path = out_path.join("tables.rs");
    let mut tables_writer = fs::File::create(tables_path)?;

    writeln!(
        tables_writer,
        "pub(crate) const ESCAPE_SYMBOL: [u8; 256] = ["
    )?;
    for i in 0..=255u8 {
        let value = escape(i);
        writeln!(tables_writer, "    {value},")?;
    }
    writeln!(tables_writer, "];")?;

    writeln!(
        tables_writer,
        "pub(crate) const ESCAPE_FLAG: [bool; 256] = ["
    )?;
    for i in 0..=255u8 {
        let value = escape(i) != 0;
        writeln!(tables_writer, "    {value},")?;
    }
    writeln!(tables_writer, "];")?;
    drop(tables_writer);

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
