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

use std::ops::AddAssign;
use uninit::prelude::*;

use crate::tables::{ESCAPE_FLAG, ESCAPE_SYMBOL};
use katcp_codec_fsm::MessageType;

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

/// A katcp message. The name and arguments can either own their data or
/// reference existing data from a buffer.
///
/// The katcp specification is byte-oriented, so the text fields are \[u8\]
/// rather than [str]. The name has a restricted character set that ensures
/// it can be decoded as ASCII (or UTF-8) but the arguments may contain
/// arbitrary bytes.
///
/// The message ID and name are *not* validated when constructed with
/// [Message::new]. Using an invalid value for either will not panic, but
/// will lead to invalid formatting from [Message::write_out].
#[derive(Clone, Debug)]
pub struct Message<N, A>
where
    N: AsRef<[u8]>,
    A: AsRef<[u8]>,
{
    /// Message type
    pub mtype: MessageType,
    /// Message name
    pub name: N,
    /// Message ID, if present. It must be positive.
    pub mid: Option<u32>,
    /// Message arguments
    pub arguments: Vec<A>,
}

impl<N, A> Message<N, A>
where
    N: AsRef<[u8]>,
    A: AsRef<[u8]>,
{
    /// Create a new message.
    pub fn new(
        mtype: MessageType,
        name: impl Into<N>,
        mid: Option<u32>,
        arguments: impl Into<Vec<A>>,
    ) -> Self {
        Self {
            mtype,
            name: name.into(),
            mid,
            arguments: arguments.into(),
        }
    }

    fn type_symbol(mtype: MessageType) -> u8 {
        match mtype {
            MessageType::Request => b'?',
            MessageType::Reply => b'!',
            MessageType::Inform => b'#',
        }
    }

    /// Write a single byte to `target` and return the remaining suffix.
    ///
    /// # Safety
    ///
    /// `target` must not be empty.
    #[inline]
    #[must_use]
    fn append_byte(target: Out<[u8]>, value: u8) -> Out<[u8]> {
        let (prefix, suffix) = target.split_at_out(1);
        prefix.get_out(0).unwrap().write(value);
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
    /// It returns any unused part of the buffer.
    ///
    /// # Panics
    ///
    /// This will panic if the target is smaller than the value returned by
    /// [write_size](Self::write_size).
    pub fn write_out<'a>(&self, mut target: Out<'a, [u8]>) -> Out<'a, [u8]> {
        target = Self::append_byte(target, Self::type_symbol(self.mtype));
        target = Self::append_bytes(target, self.name.as_ref());
        if let Some(mid) = self.mid {
            target = Self::append_byte(target, b'[');
            let mut buffer = itoa::Buffer::new();
            target = Self::append_bytes(target, buffer.format(mid).as_bytes());
            target = Self::append_byte(target, b']');
        }
        for argument in self.arguments.iter() {
            let argument = argument.as_ref();
            target = Self::append_byte(target, b' ');
            if argument.is_empty() {
                target = Self::append_bytes(target, b"\\@");
            }
            for &c in argument.iter() {
                let esc = ESCAPE_SYMBOL[c];
                if esc == 0 {
                    // No escaping is needed
                    target = Self::append_byte(target, c);
                } else {
                    target = Self::append_byte(target, b'\\');
                    target = Self::append_byte(target, esc);
                }
            }
        }
        Self::append_byte(target, b'\n')
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
        for argument in self.arguments.iter() {
            let argument = argument.as_ref();
            if argument.is_empty() {
                bytes += 2; // For the \@
            } else {
                bytes += argument.len();
                bytes += argument.iter().filter(|&&c| ESCAPE_FLAG[c]).count();
            }
        }
        bytes.0
    }

    /// Encode the message to a [Vec]
    pub fn to_vec(&self) -> Vec<u8> {
        let size = self.write_size();
        let mut vec = Vec::with_capacity(size);
        let remain = self.write_out(vec.get_backing_buffer());
        if !remain.is_empty() {
            panic!("Size of message changed during formatting.");
        }
        // SAFETY: we've verified that write_out initialized all elements.
        unsafe {
            vec.set_len(size);
        }
        vec
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use rstest::*;
    use std::cell::Cell;

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

    /// Evil Message that uses interior mutability to change length dynamically
    #[rstest]
    #[case(100)]
    #[case(-100)]
    #[should_panic]
    fn change_size(#[case] delta: isize) {
        #[derive(Clone)]
        struct EvilData {
            length: Cell<isize>,
            delta: isize,
        }

        impl EvilData {
            fn new(initial: isize, delta: isize) -> Self {
                EvilData {
                    length: Cell::new(initial),
                    delta,
                }
            }
        }

        impl AsRef<[u8]> for EvilData {
            fn as_ref(&self) -> &[u8] {
                // Change by delta every call
                let cur = self.length.get();
                self.length.set(cur + self.delta);
                return &[b'x'; 10000][..cur as usize];
            }
        }

        let message: Message<EvilData, &[u8]> = Message::new(
            MessageType::Request,
            EvilData::new(5000, delta),
            None,
            Vec::new(),
        );
        let _ = message.to_vec();
    }
}
