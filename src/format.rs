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
use uninit::prelude::*;

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

    #[inline]
    #[must_use]
    unsafe fn append_byte(target: Out<[u8]>, value: u8) -> Out<[u8]> {
        let (prefix, suffix) = target.split_at_out(1);
        prefix.get_unchecked_out(0).write(value);
        suffix
    }

    #[inline]
    #[must_use]
    fn append_bytes<'a>(target: Out<'a, [u8]>, values: &[u8]) -> Out<'a, [u8]> {
        let len = values.len();
        let (prefix, suffix) = target.split_at_out(len);
        prefix.copy_from_slice(values);
        suffix
    }

    /// Write the message into a buffer.
    ///
    /// # Safety
    ///
    /// The target must have size of at least [write_size].
    pub unsafe fn write_out(&self, mut target: Out<[u8]>) {
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
            for &c in argument.iter() {
                let esc = emap[c];
                if esc == 0 {
                    // No escaping is needed
                    target = Self::append_byte(target, c);
                } else {
                    target = Self::append_byte(target, b'\\');
                    target = Self::append_byte(target, esc);
                }
            }
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

    pub fn write_size_callback(&self) -> (usize, impl Fn(Out<[u8]>) + '_) {
        let size = self.write_size();
        let callback = move |out: Out<[u8]>| {
            if out.len() != size {
                panic!("Buffer has the wrong size");
            }
            // SAFETY: this lambda captures &self, so the length cannot change.
            unsafe {
                self.write_out(out);
            }
        };
        (size, callback)
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let size = self.write_size();
        let mut vec = Vec::with_capacity(size);
        unsafe {
            self.write_out(vec.get_backing_buffer());
            vec.set_len(size);
        }
        vec
    }
}
