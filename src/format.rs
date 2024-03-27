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
use std::ops::AddAssign;
use uninit::prelude::*;

use crate::message::{Message, MessageType};

// Accumulator that panics on overflow
struct Accumulator(usize);

impl AddAssign<usize> for Accumulator {
    fn add_assign(&mut self, rhs: usize) {
        self.0 = self
            .0
            .checked_add(rhs)
            .expect("message size should not exceed usize::MAX");
    }
}

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

    /// Write a single byte to `target` and return the remaining suffix.
    ///
    /// # Safety
    ///
    /// `target` must not be empty.
    #[inline]
    #[must_use]
    unsafe fn append_byte(target: Out<[u8]>, value: u8) -> Out<[u8]> {
        let (prefix, suffix) = target.split_at_out(1);
        prefix.get_unchecked_out(0).write(value);
        suffix
    }

    /// Write a byte slice to `target` and return the remaining suffix.
    ///
    /// # Safety
    ///
    /// `target` must be at least as large as `values`.
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
    /// The target must have size of at least [write_size](Self::write_size).
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

    /// Get the number of bytes needed by [write_out](Self::write_out).
    ///
    /// # Panics
    ///
    /// This function will panic if the size overflows [usize].
    pub fn write_size(&self) -> usize {
        let mut bytes = Accumulator(2); // name and newline
        bytes += self.name.as_ref().len();
        bytes += self.arguments.len(); // spaces between arguments
        if let Some(mid) = self.mid {
            let mut buffer = itoa::Buffer::new();
            let mid_formatted = buffer.format(mid);
            bytes += 2 + mid_formatted.len(); // 2 for the brackets
        }
        let emap = Self::escape_map();
        for argument in self.arguments.iter() {
            let argument = argument.as_ref();
            if argument.is_empty() {
                bytes += 2; // For the \@
            } else {
                bytes += argument.len();
                bytes += argument.iter().filter(|&&c| emap[c] != 0).count(); // escapes
            }
        }
        bytes.0
    }

    /// Get the size and a callback to write the message.
    ///
    /// The callback panics if the provided buffer doesn't match the returned
    /// size.
    ///
    /// # Example
    ///
    /// ```
    /// use uninit::prelude::*;
    /// # use _lib::message::{Message, MessageType};
    /// # let message: Message<&[u8], &[u8]> = Message::new(MessageType::Request, &b""[..], None, vec![]);
    ///
    /// let (size, callback) = message.write_size_callback();
    /// let mut out = vec![0u8; size];
    /// callback(out.as_out());
    /// ```
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

    /// Encode the message to a [Vec]
    pub fn to_vec(&self) -> Vec<u8> {
        let (size, callback) = self.write_size_callback();
        let mut vec = Vec::with_capacity(size);
        callback(vec.get_backing_buffer());
        // SAFETY: we've used the callback to initialize all elements.
        unsafe {
            vec.set_len(size);
        }
        vec
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Create a Message that requires more than usize bytes.
    #[test]
    #[should_panic(expected = "message size should not exceed usize::MAX")]
    fn overflow_size() {
        /// Zero-size structure that can be used as a message argument
        #[derive(Copy, Clone)]
        struct ZeroSizeArgument;

        impl AsRef<[u8]> for ZeroSizeArgument {
            fn as_ref(&self) -> &[u8] {
                b"argument".as_slice()
            }
        }

        // We need a way to construct the giant vector without iterating
        // over the elements (release builds will optimise away the useless
        // loop, but debug builds do not). Since there is no memory to
        // initialize, set_len should be safe.
        let mut arguments = vec![];
        unsafe {
            arguments.set_len(usize::MAX - 5);
        }
        let message: Message<&[u8], ZeroSizeArgument> =
            Message::new(MessageType::Request, &b"big message"[..], None, arguments);
        message.write_size();
    }
}
