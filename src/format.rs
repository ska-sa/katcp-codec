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

use enum_map::{enum_map, EnumMap};
use once_cell::sync::OnceCell;
use std::io::Write;
use std::mem::MaybeUninit;

use crate::message::{Message, MessageType};

impl<N, A> Message<N, A>
where
    N: AsRef<[u8]>,
    A: AsRef<[u8]>,
{
    fn type_symbol(mtype: MessageType) -> u8 {
        match mtype {
            MessageType::Request => b'?',
            MessageType::Reply => b'!',
            MessageType::Inform => b'#',
        }
    }

    fn escape_map() -> &'static EnumMap<u8, u8> {
        static INSTANCE: OnceCell<EnumMap<u8, u8>> = OnceCell::new();
        INSTANCE.get_or_init(|| {
            enum_map! {
                b'\r' => b'r',
                b'\n' => b'n',
                b'\t' => b't',
                b'\x1B' => b'e',
                b'\0' => b'0',
                b'\\' => b'\\',
                b' ' => b'_',
                _ => 0,  // Marker for not needing an escape
            }
        })
    }

    /// Serialize the message
    pub fn write<T: Write>(&self, target: &mut T) -> std::io::Result<()> {
        let symbol = Self::type_symbol(self.mtype);
        target.write_all(std::slice::from_ref(&symbol))?;
        target.write_all(self.name.as_ref())?;
        if let Some(mid) = self.mid {
            target.write_all(b"[")?;
            let mut buffer = itoa::Buffer::new();
            let mid_formatted = buffer.format(mid);
            target.write_all(mid_formatted.as_bytes())?;
            target.write_all(b"]")?;
        }
        for argument in self.arguments.iter() {
            let argument = argument.as_ref();
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

    #[inline]
    #[must_use]
    unsafe fn append_byte(target: &mut [MaybeUninit<u8>], value: u8) -> &mut [MaybeUninit<u8>] {
        target[0].write(value);
        target.get_unchecked_mut(1..)
    }

    #[inline]
    #[must_use]
    unsafe fn append_bytes<'a>(
        target: &'a mut [MaybeUninit<u8>],
        values: &[u8],
    ) -> &'a mut [MaybeUninit<u8>] {
        let len = values.len();
        std::ptr::copy_nonoverlapping(
            values.as_ptr(),
            target.as_mut_ptr() as *mut u8,
            values.len(),
        );
        target.get_unchecked_mut(len..)
    }

    /// Write the message into a buffer.
    ///
    /// # Safety
    ///
    /// The target must have size of at least [write_size].
    pub unsafe fn write_unchecked(&self, mut target: &mut [MaybeUninit<u8>]) {
        target = Self::append_byte(target, Self::type_symbol(self.mtype));
        target = Self::append_bytes(target, self.name.as_ref());
        if let Some(mid) = self.mid {
            target = Self::append_byte(target, b'[');
            let mut buffer = itoa::Buffer::new();
            target = Self::append_bytes(target, buffer.format(mid).as_bytes());
            target = Self::append_byte(target, b']');
        }
        let emap = Self::escape_map();
        for argument in self.arguments.iter() {
            let argument = argument.as_ref();
            target = Self::append_byte(target, b' ');
            if argument.is_empty() {
                target = Self::append_bytes(target, b"\\@");
            }
            let mut pos = 0;
            for &c in argument.iter() {
                let esc = emap[c];
                if esc == 0 {
                    // No escaping is needed
                    target[pos].write(c);
                    pos += 1;
                } else {
                    target[pos].write(b'\\');
                    target[pos + 1].write(esc);
                    pos += 2;
                }
            }
            target = &mut target[pos..];
        }
        let _ = Self::append_byte(target, b'\n');
    }

    /// Get the number of bytes needed by [write].
    pub fn write_size(&self) -> usize {
        // Type symbol, name, spaces and newline
        let mut bytes = 2 + self.name.as_ref().len() + self.arguments.len();
        if let Some(mid) = self.mid {
            let mut buffer = itoa::Buffer::new();
            let mid_formatted = buffer.format(mid);
            bytes += 2 + mid_formatted.len();
        }
        let emap = Self::escape_map();
        for argument in self.arguments.iter() {
            let argument = argument.as_ref();
            if argument.is_empty() {
                bytes += 2; // For the \@
            }
            for c in argument.iter() {
                bytes += if emap[*c] != 0 { 2 } else { 1 };
            }
        }
        bytes
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let size = self.write_size();
        let mut vec = Vec::with_capacity(size);
        unsafe {
            self.write_unchecked(vec.spare_capacity_mut());
            vec.set_len(size);
        }
        vec
    }
}
