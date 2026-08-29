# Stats Semantics and Usage Review

This document defines what `ytq stats` can honestly infer from the event log and
records observations from real usage that guide future Wrapped-style insights.

## Event Semantics

`ytq` records queue operations, not playback progress:

- `Queued` means a video ID entered the queue.
- `Watched` means `ytq next` or `ytq random` popped the video and successfully
  opened its URL.
- `Skipped` means the video was removed without being opened by ytq.

A `Watched` event does not prove that the video was completed. If a long video
is popped, partially viewed, and added again, its later `Watched` event may be a
continuation. It is indistinguishable from an intentional rewatch, so stats must
not infer either intent.

## Reporting Rules

The current stats follow these rules:

- **Videos Added** counts the first lifetime `Queued` event for each video ID.
- **Videos Re-added** counts later `Queued` events for an existing video ID.
- **Unique Videos Opened** counts only the first lifetime `Watched` event for
  each video ID.
- **Viewing Sessions** counts every `Watched` event. This is an activity count,
  not a completion or rewatch count.
- **Removed Without Open** counts `Skipped` events.
- **Queue Exits Opened/Removed** describes how items left the queue. It is not a
  completion rate.
- Video, channel, category, tag, and duration profiles use unique first opens.
- Time-of-day, busiest-day, streak, and sessions-per-week metrics use all
  viewing sessions because resumed sessions are still real activity.
- Queue-time statistics use only the first open because re-adding resets the
  queue timestamp.
- Metadata duration is reported as video duration, never as time actually
  watched.
- The old Comfort Video metric was removed because repeated opens do not reveal
  rewatch intent.

Date-filtered reports still load the complete history before classifying first
and repeated events. A video first added or opened before the selected period
therefore cannot be misclassified as new inside that period.

## Real-Usage Snapshot

Snapshot captured from the complete history at approximately 2026-08-29
09:00 EDT. Live totals continue to change as the queue is used:

| Metric | Value |
|---|---:|
| First-time additions | 23,499 |
| Re-additions | 148 |
| Unique videos opened | 236 |
| Viewing sessions | 258 |
| Additional open sessions for previously opened IDs | 22 |
| Removed without opening | 30 |
| Current queue depth | 23,356 |
| Current queue duration | About 460 days |

The 22 additional opens are intentionally described as sessions. The log does
not establish whether they were continuations or rewatches.

### Observations

- The queue is primarily a large archive. Its median item age is about 124 days,
  90% of items are at least about 209 days old, and 7,488 items are over 180
  days old.
- First opens are much more selective than additions. About 1% of first-added
  IDs have a first open recorded. This is a queue conversion measurement, not
  a completion measurement.
- Viewing is concentrated on active days: 258 sessions across 94 active days,
  or about 2.7 sessions per active day.
- First-open latency is strongly split between immediate and archival use. The
  median is about 15 hours, the 75th percentile is about 23 days, and the 90th
  percentile is about 53 days.
- Long videos are selected disproportionately often. Videos at least one hour
  long are about 12% of first additions but 22% of uniquely opened videos.
- Re-addition is uncommon overall: 148 events across 122 video IDs. Nineteen of
  those IDs have a later open, while most remain queued.
- Morning and evening are both substantial viewing periods. A single
  personality label can hide this mixed pattern.
- Channel affinity is better represented as an open rate with a minimum sample
  size than by raw queue or open counts alone. For example, channels with many
  saved videos can be compared by uniquely opened IDs divided by first-added
  IDs.
- The February history includes large imports and bulk queue activity. Trends
  should avoid presenting all additions as deliberate same-day discoveries
  because the ytq event schema does not record the source of an addition.
- The companion YouTube Tab Manager logs contain 103,764 successful add
  attempts, including 78,530 `already_in_queue` results. ytq correctly omits
  those no-op attempts from `Queued` history, so ytq queue-addition stats and
  Tab Manager processing-volume stats answer different questions.

## Candidate Wrapped Insights

These are suitable future metrics because they rely on observable facts and do
not infer completion or rewatch intent.

1. **Queue Age Profile**
   - Median and 90th-percentile queue age
   - Fresh, aging, and archive age buckets
   - Oldest queued item

2. **First-Open Funnel**
   - Unique first-added IDs versus unique first-opened IDs
   - Open rate by channel, category, and duration band
   - Minimum sample thresholds to avoid noisy rankings

3. **Duration Preference Lift**
   - Compare the duration distribution of first additions with unique opens
   - Highlight over-indexed bands such as one-hour-plus videos

4. **First-Open Latency Profile**
   - Median, 25th, 75th, and 90th percentiles
   - Same day, same week, same month, and archive-open buckets

5. **Viewing Cadence**
   - Active viewing days
   - Sessions per active day
   - Morning/evening split instead of forcing one personality label

6. **Re-add Follow-Through**
   - Re-added IDs subsequently opened
   - Re-added IDs still queued
   - No claim about whether a later open is a continuation or rewatch

7. **Backlog Trajectory**
   - Monthly first additions, re-additions, first opens, and queue-depth change
   - Estimated queue runway at the current unique-first-open rate, clearly
     labeled as a projection rather than a prediction

8. **Content Freshness**
   - Median publication year
   - Same-year versus archive content shares
   - Oldest uniquely opened video

9. **Channel and Category Affinity**
   - Open-rate rankings with minimum denominators
   - Difference between queue share and unique-open share
   - Avoid rankings based only on raw counts

## Data Needed for Stronger Claims

True completion, resume, and rewatch statistics require explicit intent that is
not currently captured. A future workflow could add an explicit completion or
resume action, but existing events must remain interpreted conservatively.
