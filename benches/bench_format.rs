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

use _lib::message::{Message, MessageType};

fn format_no_escape(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_no_escape");
    for args in [1, 10, 100, 1000, 10000] {
        let msg: Message<&[u8], &[u8]> = Message::new(
            MessageType::Request,
            b"test_message".as_slice(),
            Some(12345678),
            vec![b"123.4567890:123.4567890".as_slice(); args],
        );
        let len = msg.to_bytes().len();
        group.throughput(Throughput::Bytes(len as u64));
        group.bench_function(BenchmarkId::from_parameter(args), |b| {
            b.iter(|| msg.to_bytes());
        });
    }
    group.finish();
}

criterion_group!(benches, format_no_escape);
criterion_main!(benches);
