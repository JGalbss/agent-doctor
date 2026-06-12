// Rules mined from the EffectPatterns corpus (references/effect-patterns).
mod common;

use common::{assert_fires, assert_silent};

#[test]
fn async_callback_in_map() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.map(Effect.succeed(1), async (x) => x + 1)
"#,
        "no-async-callback-in-effect-combinators",
        1,
    );
}

#[test]
fn async_callback_in_sync() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.sync(async () => fetch("https://x"))
"#,
        "no-async-callback-in-effect-combinators",
        1,
    );
}

#[test]
fn async_in_try_promise_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.tryPromise(async () => fetch("https://x"))
"#,
        "no-async-callback-in-effect-combinators",
    );
}

#[test]
fn then_chain_in_sync() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const fetchData: () => Promise<string>
const program = Effect.sync(() => {
  fetchData().then((data) => data.length)
})
"#,
        "no-then-in-sync",
        1,
    );
}

#[test]
fn promise_all_inside_try_promise() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const ids: ReadonlyArray<string>
declare const fetchUser: (id: string) => Promise<unknown>
const program = Effect.tryPromise(() => Promise.all(ids.map(fetchUser)))
"#,
        "no-promise-all-in-effect",
        1,
    );
}

#[test]
fn bare_try_promise_suggests_typed_catch() {
    let source = r#"
import { Effect } from "effect"
const bare = Effect.tryPromise(() => fetch("https://x"))
const typed = Effect.tryPromise({ try: () => fetch("https://x"), catch: (cause) => ({ _tag: "FetchError", cause }) })
"#;
    assert_fires(source, "require-typed-catch-in-try", 1);
}

#[test]
fn run_sync_on_promise() {
    assert_fires(
        r#"
import { Effect } from "effect"
const value = Effect.runSync(Effect.promise(() => fetch("https://x")))
"#,
        "no-runsync-on-async-effect",
        1,
    );
}

#[test]
fn run_sync_on_delayed_pipe() {
    assert_fires(
        r#"
import { Effect } from "effect"
const value = Effect.runSync(Effect.succeed(1).pipe(Effect.delay("1 second"), Effect.map((n) => n)))
"#,
        "no-runsync-on-async-effect",
        1,
    );
}

#[test]
fn run_sync_on_sync_effect_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const value = Effect.runSync(Effect.succeed(1))
"#,
        "no-runsync-on-async-effect",
    );
}

#[test]
fn map_returning_effect() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.map(Effect.succeed(1), (n) => Effect.log(n))
"#,
        "no-map-returning-effect",
        1,
    );
}

#[test]
fn map_returning_value_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.map(Effect.succeed(1), (n) => n + 1)
"#,
        "no-map-returning-effect",
    );
}

#[test]
fn infinite_stream_run_collect() {
    assert_fires(
        r#"
import { Effect, Stream } from "effect"
const out = Stream.forever(Stream.succeed(1)).pipe(Stream.runCollect)
"#,
        "no-runcollect-on-infinite-stream",
        1,
    );
}

#[test]
fn infinite_stream_with_take_is_fine() {
    assert_silent(
        r#"
import { Effect, Stream } from "effect"
const out = Stream.forever(Stream.succeed(1)).pipe(Stream.take(100), Stream.runCollect)
"#,
        "no-runcollect-on-infinite-stream",
    );
}

#[test]
fn eager_chunk_stream() {
    assert_fires(
        r#"
import { Chunk, Effect, Stream } from "effect"
declare const big: Iterable<number>
const stream = Stream.fromChunk(Chunk.fromIterable(big))
"#,
        "no-eager-chunk-stream",
        1,
    );
}

#[test]
fn map_effect_without_concurrency() {
    let source = r#"
import { Effect, Stream } from "effect"
declare const work: (n: number) => ReturnType<typeof Effect.succeed<number>>
const a = Stream.mapEffect(work)
const b = Stream.mapEffect(work, { concurrency: 4 })
"#;
    assert_fires(source, "stream-mapeffect-missing-concurrency", 1);
}

#[test]
fn unbounded_queue() {
    let source = r#"
import { Effect, Queue, PubSub } from "effect"
const q = Queue.unbounded<string>()
const p = PubSub.unbounded<string>()
const ok = Queue.bounded<string>(100)
"#;
    assert_fires(source, "prefer-queue-bounded", 2);
}

#[test]
fn try_finally_with_yields_in_gen() {
    assert_fires(
        r#"
import { Effect, Fiber } from "effect"
declare const poller: ReturnType<typeof Effect.succeed<number>>
declare const job: ReturnType<typeof Effect.succeed<number>>
const program = Effect.gen(function* () {
  const fiber = yield* Effect.fork(poller)
  try {
    return yield* job
  } finally {
    yield* Fiber.interrupt(fiber)
  }
})
"#,
        "no-try-finally-in-gen",
        1,
    );
}

#[test]
fn try_finally_without_yields_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  let handle = 0
  try {
    handle = 1
  } finally {
    handle = 0
  }
  return yield* Effect.succeed(handle)
})
"#,
        "no-try-finally-in-gen",
    );
}

#[test]
fn object_literal_comparison() {
    let source = r#"
import { Effect } from "effect"
declare const selected: ReadonlyArray<{ id: number }>
declare const current: { id: number }
const a = current === { id: 1 }
const b = selected.includes({ id: 1 })
export const noop = Effect.void
"#;
    assert_fires(source, "no-object-literal-comparison", 2);
}

#[test]
fn tag_comparison_against_builtin() {
    let source = r#"
import { Effect, Either } from "effect"
declare const result: ReturnType<typeof Either.left<string>>
if (result._tag === "Left") {
  // handle
}
export const noop = Effect.void
"#;
    assert_fires(source, "no-tag-string-comparison", 1);
}

#[test]
fn custom_tag_comparison_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
declare const event: { _tag: string }
if (event._tag === "OrderPlaced") {
  // handle
}
export const noop = Effect.void
"#,
        "no-tag-string-comparison",
    );
}

#[test]
fn switch_on_tag() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const event: { _tag: "A" | "B" }
switch (event._tag) {
  case "A":
    break
  case "B":
    break
}
export const noop = Effect.void
"#,
        "prefer-match-over-tag-switch",
        1,
    );
}

#[test]
fn fail_with_string() {
    let source = r#"
import { Effect } from "effect"
const a = Effect.fail("Something went wrong!")
const b = Effect.tryPromise({ try: () => fetch("https://x"), catch: () => "FetchError" })
"#;
    assert_fires(source, "no-string-errors", 2);
}

#[test]
fn fail_with_tagged_error_is_fine() {
    assert_silent(
        r#"
import { Data, Effect } from "effect"
class QueryError extends Data.TaggedError("QueryError")<{ cause: unknown }> {}
const a = Effect.fail(new QueryError({ cause: null }))
"#,
        "no-string-errors",
    );
}

#[test]
fn catch_all_to_null() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const getUser: ReturnType<typeof Effect.succeed<string>>
const user = getUser.pipe(Effect.catchAll(() => Effect.succeed(null)))
"#,
        "no-catchall-to-null",
        1,
    );
}

#[test]
fn effect_all_without_concurrency() {
    let source = r#"
import { Effect } from "effect"
const a = Effect.all([Effect.succeed(1), Effect.succeed(2)])
const b = Effect.all([Effect.succeed(1), Effect.succeed(2)], { concurrency: 2 })
"#;
    assert_fires(source, "effect-all-missing-concurrency", 1);
}

#[test]
fn race_against_sleep() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const fetchData: ReturnType<typeof Effect.succeed<string>>
const result = Effect.race(fetchData, Effect.sleep("2 seconds"))
"#,
        "prefer-timeout-over-race-sleep",
        1,
    );
}

#[test]
fn fork_then_immediate_join() {
    assert_fires(
        r#"
import { Effect, Fiber } from "effect"
declare const task: ReturnType<typeof Effect.succeed<number>>
const program = Effect.gen(function* () {
  const fiber = yield* Effect.fork(task)
  const result = yield* Fiber.join(fiber)
  return result
})
"#,
        "no-fork-then-immediate-join",
        1,
    );
}

#[test]
fn fork_with_work_between_is_fine() {
    assert_silent(
        r#"
import { Effect, Fiber } from "effect"
declare const task: ReturnType<typeof Effect.succeed<number>>
const program = Effect.gen(function* () {
  const fiber = yield* Effect.fork(task)
  yield* Effect.logInfo("working")
  const result = yield* Fiber.join(fiber)
  return result
})
"#,
        "no-fork-then-immediate-join",
    );
}

#[test]
fn raw_millis_durations() {
    let source = r#"
import { Effect, Schedule } from "effect"
const a = Effect.sleep(2000)
const b = Schedule.spaced(500)
const ok = Effect.sleep("2 seconds")
const factorOk = Schedule.exponential("100 millis", 2)
"#;
    assert_fires(source, "prefer-duration-over-raw-millis", 2);
}

#[test]
fn sync_wrapping_literal() {
    let source = r#"
import { Effect } from "effect"
const a = Effect.sync(() => 42)
const lazyOk = Effect.sync(() => globalThis.performance.now())
"#;
    assert_fires(source, "prefer-succeed-over-sync-literal", 1);
}

#[test]
fn config_string_secret() {
    let source = r#"
import { Config, Effect } from "effect"
const apiKey = Config.string("API_KEY")
const password = Config.string("DB_PASSWORD")
const host = Config.string("DB_HOST")
"#;
    assert_fires(source, "prefer-config-redacted", 2);
}

#[test]
fn json_stringify_in_log() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const results: ReadonlyArray<number>
const program = Effect.log(`Results: ${JSON.stringify(results)}`)
"#,
        "prefer-structured-logging-args",
        1,
    );
}

#[test]
fn text_response_with_stringify() {
    assert_fires(
        r#"
import { Effect } from "effect"
import { HttpServerResponse } from "@effect/platform"
declare const user: { id: string }
const response = HttpServerResponse.text(JSON.stringify(user))
"#,
        "prefer-json-response-helper",
        1,
    );
}

#[test]
fn long_flatmap_chain() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const start: ReturnType<typeof Effect.succeed<number>>
declare const step: (n: number) => ReturnType<typeof Effect.succeed<number>>
const program = start.pipe(
  Effect.flatMap(step),
  Effect.flatMap(step),
  Effect.andThen(step),
  Effect.flatMap(step)
)
"#,
        "avoid-long-combinator-chains",
        1,
    );
}

#[test]
fn layer_mergeall_megalist() {
    assert_fires(
        r#"
import { Layer, Effect } from "effect"
declare const L: Layer.Layer<never>
const app = Layer.mergeAll(L, L, L, L, L, L, L, L, L, L, L, L)
export const noop = Effect.void
"#,
        "no-layer-mergeall-megalist",
        1,
    );
}

#[test]
fn node_http_import() {
    assert_fires(
        r#"
import * as http from "node:http"
import { Effect } from "effect"
export const noop = Effect.void
"#,
        "prefer-node-effect-counterparts",
        1,
    );
}
