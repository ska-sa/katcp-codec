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

use std::io::Write;

use crate::message::{Message, MessageType};

impl Message<'_> {
    /// Serialize the message
    pub fn write<T: Write>(&self, target: &mut T) -> std::io::Result<()> {
        let type_symbol = match self.mtype {
            MessageType::Request => b'?',
            MessageType::Reply => b'!',
            MessageType::Inform => b'#',
        };
        target.write_all(std::slice::from_ref(&type_symbol))?;
        target.write_all(&self.name)?;
        if let Some(mid) = self.mid {
            target.write_all(b"[")?;
            let mut buffer = itoa::Buffer::new();
            let mid_formatted = buffer.format(mid);
            target.write_all(mid_formatted.as_bytes())?;
            target.write_all(b"]")?;
        }
        for argument in self.arguments.iter() {
            target.write_all(b" ")?;
            if argument.is_empty() {
                target.write_all(b"\\@")?;
            }
            let mut buf = [0u8; 2];
            for &c in argument.iter() {
                buf[1] = match c {
                    b'\r' => b'r',
                    b'\n' => b'n',
                    b'\t' => b't',
                    b'\x1B' => b'e',
                    b'\0' => b'0',
                    b'\\' => b'\\',
                    b' ' => b'_',
                    _ => b'\0', // Indicate no escaping is needed
                };
                if buf[1] == b'\0' {
                    buf[0] = c;
                    target.write_all(&buf[..1])?;
                } else {
                    buf[0] = b'\\';
                    target.write_all(&buf[..2])?;
                }
            }
        }
        target.write_all(b"\n")?;
        Ok(())
    }

    /// Get the number of bytes needed by [write].
    pub fn write_size(&self) -> usize {
        // Type symbol, name, spaces and newline
        let mut bytes = 2 + self.name.len() + self.arguments.len();
        if let Some(mid) = self.mid {
            let mut buffer = itoa::Buffer::new();
            let mid_formatted = buffer.format(mid);
            bytes += 2 + mid_formatted.len();
        }
        for argument in self.arguments.iter() {
            if argument.is_empty() {
                bytes += 2; // For the \@
            }
            for c in argument.iter() {
                bytes += match c {
                    b'\r' | b'\n' | b'\t' | b'\x1B' | b'\0' | b'\\' | b' ' => 2,
                    _ => 1,
                }
            }
        }
        bytes
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec = Vec::with_capacity(self.write_size());
        self.write(&mut vec).unwrap(); // write to vec cannot fail
        vec
    }
}
