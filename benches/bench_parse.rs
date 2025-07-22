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

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use _lib::format::Message as FormatMessage;
use _lib::message::MessageType;
use _lib::parse::Parser;

fn parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for escapes in [false, true] {
        let arg_value = if escapes {
            b"[1, 2, 3, 4, 5, 6, 7, 8]".as_slice()
        } else {
            b"123.4567890:123.45678901".as_slice()
        };
        for args in [1, 10, 100, 1000, 10000] {
            let msg: FormatMessage<&[u8], &[u8]> = FormatMessage::new(
                MessageType::Request,
                b"test_message".as_slice(),
                Some(12345678),
                vec![arg_value; args],
            );
            let encoded = msg.to_vec();
            let mut parser = Parser::new(usize::MAX);
            group.throughput(Throughput::Bytes(encoded.len() as u64));
            let name = if escapes { "escapes" } else { "no escapes" };
            group.bench_function(BenchmarkId::new(name, args), |b| {
                b.iter(|| parser.append(&encoded).count());
            });
        }
    }
    group.finish();
}

criterion_group!(benches, parse);
criterion_main!(benches);
