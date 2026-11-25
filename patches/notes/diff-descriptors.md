# N3 — descriptor/view comparison (initial static audit)

Owner #101, helpers R1 #11 and G2 #30, phase #93. Pinned [Comet 71c03dc](https://github.com/Fyntra-Development/Comet/tree/71c03dc47d4db41bd03433fdd4217af5996277ff), [Belobug 4cbe9d7](https://github.com/Belobug/VIctory-Belobug-AMD/tree/4cbe9d7), [hyphennn a26859f](https://github.com/fxceless/victory-fork/tree/a26859f). All source citations below refer to `crates/{victory,comet}-gpu/src/` at these revisions. B = Belobug, H = hyphennn, C = Comet; D = descriptor.rs, DH = descriptor/host.rs. This is a source audit, not a completed shader attribution or empirical causal conclusion.

Blob fingerprints:
- 4cbe9d7: D a8e14d9f04de0710b66c2f193f0277873c29e428, DH 51b3d192baf44c4891c916909eb94fafccfad1b6
- a26859f: D 6aad6da0349a65e329cf0275ed2ea45cf62a61bd, DH 861106a1f4a77ad0cd8167a6d5ce7e70946d4a01
- 71c03dc47d4db41bd03433fdd4217af5996277ff: D c191dbf8d14b6909e262a22066714ff120348a91, DH ca2bff774942a02a908cf98a7178a7f34245f4a0

## Measured symptom and evidence boundaries

#44/#51: after Phase 1 (e82716d) plus ambient ff08f59, black albedo ~39–42% (formerly 81%); textures waiting ~2500–3000; bunks have normals but zero albedo; walls/floor locally absent from depth. #93/#11/#30: exact post-mip-skip clothing and bunk draw hashes not supplied. The six measured failing material PS hashes in docs/missing-world.md:23–25 are 7d0f14ce44746d7e (8 dropped draws/frame), 6b6db9f1a526daeb (3), 8660e9822ebe4b62, abc6a80d46edc294, c8df7216c69cecbd, 2fd01fca16993001 ("descriptor read only as data"). These are **not attributed to the bunk or clothing** and a host descriptor repair does not translate those shaders. Known compute descriptor-as-data failure ceb5eb68cd99cb07 is also not a descriptor host fix. L5/N1 own shader translation. No exact bunk material hash or current run log proving the proposed repair has executed is available; request below.

## Behavioral differences and dispositions

| Priority / source behavior | Reference location | Comet location | Symptom it could explain / proposed port |
|---|---|---|---|
| P0 — off-element host buffer views get their own byte range (SRV and UAV); both binding and refresh select same copy | H DH:2093–2119,2143–2215,2275–2298,2363–2392; H shadow.rs:3589–3663; B DH:2187–2231,2268–2327 uses shadow::for_view instead | C DH:1746–1761,1842–1878,1914–1952; C shadow.rs:3434–3477 (no exact-range allocator) | Source-proven refusal (existing diagnostic strings: `buffer view not on an element of its host copy`, `unordered buffer view not on an element of its host copy`); null reads could suppress material index, colour bake constants, or culling data. H shadow.rs:3603–3606 says upstream 86a5ed6 identified meshlet culling constants and colour-bake parameter blocks, not our measured draw. Stage bounded exact-range copy; same range for creation and refresh/write-back. |
| P0 — raw descriptor byte copies discovered at sync | H D:172–287, DH:1798–1840 | C D:129–172, DH:1442–1539 | Bindless material slot could remain unmapped despite valid texture. Benchmark owner's sync_heaps experiment, do NOT duplicate. |
| P0 — packed mip neighbours and exact start replacement rule, next-start and first viewed level tracking | H DH:95–206,241–352,429–442 | C DH:218–318,352–360; L1 packed-head gate | Wrong stale-out can zero a material texture that was resident. Compare mapped mip head and adjacent span; existing L1 gate is partial counterpart, coordinate B3/N5. |
| P0 — budget-denied texture slots parked until room, per-level wait/remake | H DH:125–152,623–642,657–690,1286–1410 | C DH:425–577,1262–1397 | Full-card backlog and null albedo; do not duplicate #82–86, measure queue/capacity and parked count. |
| P1 — page-write dirty notification and uploaded texture recheck | H DH:363–410,1246–1285,1331–1374,657–666 | C DH:320–350,561–570,1322–1360 | Streaming title can overwrite same bytes without re-creating view; stale/missing colour. Require mapped-page log and compare texture data; N5/N6 coordinate page-watch path. |
| P1 — bindless slots cached plan, refresh, CBV bounce, placed textures recording and revalidation | B DH:149–208,246–271,352–454,1097–1330,1342–1567,1574–1916 | C DH:1031–1247,1296–1397,1442–1539,1746–1761 | Read-after-write/misaligned CBV can select wrong colour index; B architecture differs substantially. Port only proven missing components after R1/G2 root-indexed captures; compare actual slot and refresh range. |
| P1 — pool ownership retained with descriptor and list | B DH:37–39,924–969 | C DH:1281–1295,1539–1551 | Prevent pooled GPU allocation reuse behind live view; N5 owns pool work. Do not import allocator assumptions into Comet. |
| P1 — released-view bulk stale invalidation and memory-remap epoch | B D:405–443; B DH:280–336,1132–1163,1574–1814 | C D:731–758; C DH:1043–1061,1322–1360,1442–1470 | A stale resource/view may read a reused texture; need capture of released slot, not assumed cause. |
| P1 — mip identity and view adjustment | B DH:2415–2451,2583–2607; H DH:95–123 | C DH:1579–1639,2118–2296 | Comet already has mip-skip adjustment and B3 credited earlier mip port; H first-viewed level and next-span may still matter. Compare actual mip levels and mapped ranges; N5/B3. |
| P1 — bindless stride AND count table | B DH:67–125,970–990,2086–2132 | C DH:161–165,626–662,1221–1246 | Counts/strides can affect byte-address load bounds; paired shader ABI change, N1 coordination. AMD compiler workaround alone not justified on NVIDIA. |
| P1 — typed/null-kind descriptor based on shader expected resource | B DH:877–920 | C DH:964–990,2333–2477 | Wrong null type can cause device error or wrong sample, not demonstrated material loss. Compare null slot and translated resource metadata. |
| P1 — validation of title address before bindless copy | B DH:1978–2003 | C DH:1746–1761,1842–1952; shadow.rs:3466–3494 | Invalid guest address should not be dereferenced. Port only exact check appropriate for Comet's memory/watch API. |
| P2 — heap limits fixed before shader translation on both vendors, rather than clamp only on portable-cap failure | H DH:793–845,860–935 | C DH:694–710,729–764 | Out-of-range view can read wrong/null or hang. On NVIDIA Comet already probes portable limit; test actual limit and cap before changing, no AMD-only work. |
| P2 — dynamic allocator early refusal using free count / failed length | H DH:38–55,1053–1103,1118–1125 | C DH:24–86,118–121,891–944,956–963 | CPU cost and no-free slot diagnostics; Comet already has opt-in packed allocator and same 2^18 reserve; not a colour fix without recorded allocation drops. |
| P2 — capped sampler heap with separate dynamic reserve | B DH:516–531,533–608,767–845 | C DH:729–764,891–944 | Oversized sampler tables can be unavailable; check real title count and sampler failure before port. |
| P2 — raw conversion differs: B uses raw_stride >=16 multiple of 16 plus for_view, H/C use valid raw byte window whenever possible | B DH:2149–2235,2268–2343; H DH:2227–2273 | C DH:1794–1840,1842–1983 | H raw-window matches C; don't regress NVIDIA raw reads. B checks raw stride and bounds differently; B AMD typed/structured behavior not blindly portable. |
| P2 — raw/typed validation and component mapping | H DH:2245–2273,2345–2355; B DH:2187–2262 | C DH:1812–1840,1883–1906 | C/H already have buffer_view_ok, default component mapping and refusal diagnostic; B has simpler fits_buffer. Preserve stricter C checks. |
| P2 — buffer extent refresh | H DH:2060–2119 | C DH:1713–1761 | Already default-on in C, aside from H own-range branch; do not duplicate. |
| P2 — generation-based descriptor records vs inline shapes | B D:343–443; H D:460–688 | C D:347–584 | B stores each view as indexed record and bulk stale check, C/H inline compact constants and placed shapes. C has opt-in shared views; H RwLock and entry_of. CPU/record lifetime differences, not demonstrated zero colour. |
| P2 — separate host heap Arc access | B D:85–120 | C D:69–128 | B bindless plan uses heap Arc ownership; no direct equivalent needed without plan port. |
| P2 — padding normalization | B D:502 onward | C D:785 onward | Review descriptor word normalization and equality; no actual mismatch demonstrated. Preserve C inline key layout. |
| P2 — typed texture format matching/twins and null dimensions | H DH:2465–2589,2794–2995; B DH:2354–2414,2452–2582,2626–2791 | C DH:1993–2117,2298–2498 | C/H already implement format fit, depth twins and shape-aware nulls; B different twin integration. B3 handles depth/stencil; no duplicate port. |
| P2 — descriptor telemetry and bounded texture queue scheduling | B DH:211–243,1536–1567,1923–1977; H DH:567–592,1923–1986 | C DH:478–528,1541–1578 | C bounded queue favors newest; reference policy and reporting differ. Not sufficient to claim improved FPS or residency. |

## Top staged candidate

`patches/n3-view-own-copy.patch.txt` (`COMET_VIEW_OWN_COPY=1`, otherwise OFF). Ported/adapted from fxceless/victory-fork a26859f crates/victory-gpu/src/{descriptor/host.rs,shadow.rs} (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by hyphennn). Credits hunk included. Host-only; no shader cache key. Scope: offset not a whole element, cannot use existing raw_window, nonempty valid in-bounds range; checked arithmetic. Separate range-keyed Shadow cache capped at 1024 copies/256 MiB, holds live slots/lists. Reference uses age-based eviction, ours hard cap and unheld eviction; deliberately does not implement raw-aligned-but-not-16-byte case, out-of-bounds views, invalid formats, or descriptor-byte mirroring. Exact byte representation within each copy, **not proven alias coherence** if overlapping distinct ranges share GPU writes before copy-back. V2/V3 must review this and the bindless refresh path. Keep OFF if no demonstrated colour or correctness benefit. A proposed log, not a measured one: `N3 own-view shadow allocated` (start,len,size). Allocation alone is not evidence of shader use; capture material binding plus refresh/copy-back and actual output. Test `own_view_range_is_exact_and_preserves_the_existing_raw_path` is included; build gate must compile, test and check.

## Benchmark request

Use post-mip-skip/ambient same scene, Relay off, builder off, same raw heap sync setting on both A/B runs. Baseline switch unset then switch=1. `COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300 COMET_DUMP_TARGETS=1`. Share frame ops lines with actual bunk and Lucia clothing PS/VS/mesh hashes, pipeline, RTV/DSV, root/indexed SRV and UAV ranges, exact material pixel and matching normal/albedo/stencil samples; existing baseline refusal diagnostics and candidate allocation log with title address/range (can match view to ops/root binding or instrumentation). If no refusal/own allocation in those draws, do not claim this fixes them; separate the measured shader failures above and R1/G2 root-indexed SRV repair. Compare black-albedo fraction, texture waiting, FPS, memory, device removal, no regression of other materials or culling. Capture requested on #101 and #93.

## Completeness

Both descriptor files were retrieved at all three pinned revisions and the function-locator census below covers same-named/unique functions, including tests. The behavioral table groups the inspected mechanisms, but this is **not an exhaustive proof of every semantic difference**: detailed per-function review of B's substantially divergent bindless plan, cross-module ownership, texture backing/levels, descriptor layout/padding and memory-watch contracts remains open. No speculative port promoted from those rows. The comparison identifies the next concrete candidate, not a completed visual fix.

### descriptor.rs — function locator index

Each entry is a line of the pinned file, not a claim that similarly named functions are equivalent. `—` means no same-named function (could have a renamed counterpart). This is a mechanical cross-check including tests; semantic mapping above is incomplete where explicitly marked.

| Function | Belobug | hyphennn | Comet |
|---|---:|---:|---:|
| `heap` | 64 | 68 | 64 |
| `samplers` | 71 | 75 | 71 |
| `host` | 79 | 83 | 79 |
| `host_arc` | 85 | — | — |
| `host_arc_ref` | 90 | — | — |
| `index` | 121 | 114 | 110 |
| `mirror` | 142 | 146 | 142 |
| `release_dynamic` | 171 | 278 | 161 |
| `drop` | 183 | 290 | 173 |
| `increment` | 192 | 299 | 185 |
| `desc` | 217 | 324 | 210 |
| `depth_desc` | 226 | 333 | 219 |
| `host_desc` | 237 | 344 | 230 |
| `is_buffer` | 322 | 428 | 314 |
| `resources` | 362 | 718 | 614 |
| `record` | 371 | 823 | 719 |
| `forget` | 405 | 835 | 731 |
| `stale_views` | 430 | — | — |
| `view_at` | 446 | 863 | 759 |
| `view` | 453 | 877 | 772 |
| `words` | 465 | 890 | 785 |
| `clear_padding` | 502 | — | — |
| `rtv_record` | 625 | 1030 | 925 |
| `dsv_record` | 630 | 1035 | 930 |
| `tagged_record` | 634 | 1039 | 934 |
| `create` | 740 | 1146 | 1037 |
| `heaps_hold_count_descriptors_from_their_start` | 746 | 1180 | 1043 |
| `views_survive_copies` | 772 | 1258 | 1069 |
| `views_die_with_their_resource` | 808 | 1369 | 1180 |
| `placed_heaps_start_at_the_title_memory` | 827 | 1388 | 1199 |
| `handle` | — | 108 | 104 |
| `shader_slot` | — | 133 | 129 |
| `sync_heaps` | — | 172 | — |
| `sync_line` | — | 243 | — |
| `changed_slots` | — | 261 | — |
| `inline` | — | 513 | 411 |
| `kind_words` | — | 523 | 421 |
| `payload` | — | 528 | 426 |
| `constant_words` | — | 537 | 435 |
| `constant_of` | — | 552 | 450 |
| `shape_of` | — | 560 | 458 |
| `placed_words` | — | 572 | 470 |
| `shape` | — | 579 | 477 |
| `placed_of` | — | 593 | 491 |
| `entry_of` | — | 605 | — |
| `entry` | — | 612 | 504 |
| `words_for` | — | 639 | 531 |
| `views_read` | — | 671 | — |
| `views_write` | — | 675 | — |
| `views_kept` | — | 688 | 584 |
| `waiting_texture` | — | 730 | 626 |
| `texture_made` | — | 760 | 656 |
| `wait_for` | — | 770 | 666 |
| `make_waiting` | — | 796 | 692 |
| `raw_heap_sync_stops_before_dynamic_host_slots` | — | 1153 | — |
| `records_copied_as_bytes_are_made_at_the_next_sync` | — | 1209 | — |
| `constant_views_fit_their_records` | — | 1295 | 1106 |
| `views_at_new_addresses_add_no_entries` | — | 1317 | 1128 |
| `shared_views` | — | — | 369 |
| `with_views` | — | — | 377 |

### descriptor/host.rs — function locator index

Each entry is a line of the pinned file, not a claim that similarly named functions are equivalent. `—` means no same-named function (could have a renamed counterpart). This is a mechanical cross-check including tests; semantic mapping above is incomplete where explicitly marked.

| Function | Belobug | hyphennn | Comet |
|---|---:|---:|---:|
| `new` | 79, 533 | 860 | 25, 729 |
| `drop` | 126, 1365 | — | — |
| `budget` | 227 | — | — |
| `bindless_memory_changed` | 294 | — | — |
| `mapped_since` | 308 | — | — |
| `meets` | 329 | — | — |
| `bindless_viewed_changed` | 336 | — | — |
| `copy_constants` | 385 | — | — |
| `copy_bounces` | 416 | — | — |
| `merged` | 454 | — | — |
| `near_heaps` | 479 | 776 | 663 |
| `view_slots` | 523 | — | — |
| `with` | 609 | — | — |
| `memory` | 676 | — | — |
| `fill_null` | 687 | 991 | 829 |
| `null_sampler` | 728, 734 | 1029, 1035 | 867, 873 |
| `budget_left` | 751 | — | — |
| `budget_spend` | 759 | — | — |
| `dynamic` | 767 | 1053 | 891 |
| `count` | 825 | 1107 | 945 |
| `release` | 833 | 1626 | 1247 |
| `make_at` | 846 | 1642 | 1262 |
| `make_null` | 865 | 1651 | 1271 |
| `make_null_kind` | 877 | — | — |
| `keep` | 924 | 1661 | 1281 |
| `keep_with_view` | 928 | — | — |
| `keep_owner` | 942 | — | — |
| `set_stride` | 970 | — | — |
| `set_stride_count` | 977 | — | — |
| `stride_table_gpu` | 991 | — | — |
| `constant_at` | 997 | 1777 | 1397 |
| `cpu` | 1012 | 1791 | 1411 |
| `mirror` | 1022 | 1840 | 1421 |
| `track_bindless` | 1097 | — | — |
| `untrack_bindless` | 1117 | — | — |
| `drop_bindless` | 1132 | — | — |
| `clear_stale_slot` | 1147 | — | — |
| `set_unmapped_null` | 1155 | — | — |
| `ensure_placed_slot` | 1164 | — | — |
| `ensure_buffer_slot` | 1193 | — | — |
| `ensure_cbv_bounce` | 1214 | — | — |
| `materialize_bindless_cbv` | 1256 | — | — |
| `prepare_bindless_record` | 1342 | — | — |
| `resolve_placed` | 1496 | — | — |
| `upload_due` | 1536 | — | — |
| `bindless_plan` | 1574 | — | — |
| `use_slot` | 1815 | — | — |
| `report_slot` | 1923 | — | — |
| `describe_slot` | 1957 | 1963 | 1551 |
| `plausible` | 1978 | — | — |
| `valid_title_address` | 1989 | — | — |
| `srv_desc` | 2004 | 1987 | 1640 |
| `uav_desc` | 2015 | 1998 | 1651 |
| `element_size` | 2029 | 2012 | 1665 |
| `buffer_range` | 2043 | 2026 | 1679 |
| `uav_element_size` | 2072 | — | — |
| `view_stride` | 2086 | 739 | 626 |
| `view_count` | 2105 | — | — |
| `unordered_range` | 2133 | 2123 | 1764 |
| `raw_start_fits` | 2149 | — | — |
| `raw_window` | 2169 | 2227 | 1794 |
| `raw_stride` | 2183 | — | — |
| `buffer_srv` | 2187 | 2275 | 1842 |
| `element_size_of` | 2245 | — | — |
| `fits_buffer` | 2252 | — | — |
| `buffer_uav` | 2268 | 2363 | 1914 |
| `fits` | 2354 | 2465 | 1993 |
| `without_mips` | 2415 | — | — |
| `make` | 2452 | 2590 | 2118 |
| `largest_mip` | 2583 | — | — |
| `placed_texture` | 2608 | 2814 | 2317 |
| `null_for` | 2626 | 2830 | 2333 |
| `null_uav_desc` | 2681 | 2885 | 2388 |
| `null_buffer_view` | 2719 | 2923 | 2426 |
| `null_like` | 2742 | 2946 | 2449 |
| `null_view` | 2771 | 2975 | 2478 |
| `make_shader_heap` | 2823 | — | — |
| `make_title_buffer` | 2843 | — | — |
| `compile_probe_cs` | 2876 | — | — |
| `null_uavs_keep_the_titles_dimension` | 2950 | 3002 | 2505 |
| `null_3d_uav_is_accepted_by_the_device` | 2969 | 3070 | 2550 |
| `dynamic_slots_are_held_until_released` | 2984 | 3460 | 2920 |
| `large_sampler_heap_is_capped` | 3001 | — | — |
| `small_sampler_heap_has_no_dynamic_slots` | 3018 | — | — |
| `structured_views_off_an_element_are_raw_views_of_the_same_bytes` | 3030 | — | — |
| `structured_views_off_an_element_read_and_write_their_bytes` | 3054 | — | — |
| `bindless_slots_materialize_refresh_and_copy_back_on_gpu` | 3212 | — | — |
| `bindless_plan_is_remade_only_when_slots_change` | 3458 | — | — |
| `bindless_texture_uploads_are_throttled` | 3510 | — | — |
| `bindless_heaps_queue_once_per_list` | 3540 | — | — |
| `placed_textures_evict_on_title_memory_remapped` | 3563 | — | — |
| `next_start` | — | 104, 307 | — |
| `first_viewed` | — | 116, 301 | — |
| `levels_changed` | — | 132 | — |
| `levels_waiting` | — | 150 | — |
| `bytes` | — | 213 | 221 |
| `overlapping` | — | 226 | 231 |
| `note` | — | 241 | 246 |
| `shows` | — | 296, 1935 | 1541 |
| `slots_at` | — | 312 | — |
| `forget` | — | 317 | 288 |
| `slots_in` | — | 331 | 301 |
| `slots_holding` | — | 344 | — |
| `page_written` | — | 387 | — |
| `stale_rule` | — | 434 | — |
| `remapped` | — | 449 | 369 |
| `tabled_out` | — | 498 | 418 |
| `written` | — | 544 | 455 |
| `remove` | — | 553 | 464 |
| `due` | — | 567 | 478 |
| `same` | — | 593 | 529 |
| `done` | — | 598 | 534 |
| `later` | — | 606 | 542 |
| `hold` | — | 623 | — |
| `room` | — | 632 | — |
| `partly` | — | 646 | 551 |
| `recheck` | — | 657 | — |
| `checked` | — | 666 | — |
| `changed` | — | 674 | 561 |
| `mapped` | — | 683 | 570 |
| `census` | — | 690 | 577 |
| `is_texture_view` | — | 709 | 596 |
| `device_removed` | — | 724 | 611 |
| `probe_view_limit` | — | 816 | 694 |
| `strides_gpu` | — | 1113 | 951 |
| `strides_buffer` | — | 1118 | 956 |
| `texture_slots_due` | — | 1126 | 964 |
| `texture_slots_all_due` | — | 1135 | — |
| `texture_slot_done` | — | 1149 | 973 |
| `texture_slot_recheck` | — | 1159 | — |
| `texture_slot_checked` | — | 1174 | — |
| `texture_slot_later` | — | 1181 | 984 |
| `texture_slot_held` | — | 1187 | — |
| `texture_slots_room` | — | 1192 | — |
| `texture_slot_mapped` | — | 1198 | 991 |
| `texture_slots_remapped` | — | 1212 | 1005 |
| `texture_slots_page_written` | — | 1246 | — |
| `slots_written_under` | — | 1257 | — |
| `texture_slot_made_from` | — | 1286 | — |
| `texture_slots_waiting` | — | 1331 | — |
| `texture_views_remade` | — | 1375 | — |
| `texture_slot_gone` | — | 1411 | 1031 |
| `unmake` | — | 1423 | 1043 |
| `texture_slot_tabled` | — | 1442 | 1062 |
| `tabled_in` | — | 1447 | 1067 |
| `tabled_lately` | — | 1475 | 1095 |
| `tabled_stamp` | — | 1482 | 1102 |
| `queue_texture` | — | 1491 | 1111 |
| `buffer_slots` | — | 1515 | 1141 |
| `buffer_changes` | — | 1522 | 1148 |
| `note_buffer` | — | 1535 | 1161 |
| `raw_at` | — | 1676 | 1296 |
| `null_raw_at` | — | 1702 | 1322 |
| `raw_uav_at` | — | 1711 | 1331 |
| `null_raw_uav_at` | — | 1741 | 1361 |
| `gpu_handle` | — | 1767 | 1387 |
| `mirror_copied` | — | 1811 | — |
| `made` | — | 1923 | — |
| `texture_slot_state` | — | 1944 | — |
| `buffer_extent` | — | 2060 | 1713 |
| `view_extent` | — | 2077 | 1730 |
| `buffer_copy` | — | 2093 | 1746 |
| `own_copy_on` | — | 2146 | — |
| `own_bytes` | — | 2161 | — |
| `own_bytes_if` | — | 2166 | — |
| `buffer_fields` | — | 2189 | — |
| `buffer_view_ok` | — | 2245 | 1812 |
| `twin_uav` | — | 2532 | 2060 |
| `keep_views` | — | 2795 | 2298 |
| `same_placed_texture` | — | 2803 | 2306 |
| `spans_say_the_next_start_and_the_lowest_level_viewed` | — | 3023 | — |
| `views_of_one_placed_texture_are_told_by_address_and_description` | — | 3045 | 2525 |
| `a_structured_view_made_raw_passes_the_host_check` | — | 3085 | 2565 |
| `the_texture_queue_serves_the_newest_writes_first` | — | 3107 | 2587 |
| `slots_held_for_lack_of_room_come_back_when_there_is_room` | — | 3164 | — |
| `packed_neighbours_stay` | — | 3196 | — |
| `a_texture_written_at_the_start_of_another_replaces_it` | — | 3220 | — |
| `a_slot_rewritten_moves_to_its_new_span` | — | 3245 | — |
| `memory_changed_under_packed_neighbours_names_the_ones_there` | — | 3266 | — |
| `a_write_at_the_start_of_a_packed_texture_names_it_alone` | — | 3288 | — |
| `a_page_write_makes_an_uploaded_texture_slot_due_when_asked` | — | 3312 | — |
| `never_stale_leaves_overlapping_views_out` | — | 3344 | — |
| `a_newer_view_of_other_memory_makes_the_older_stale` | — | 3356 | 2682 |
| `remapped_memory_makes_its_texture_slots_due` | — | 3392 | 2767 |
| `slots_read_through_tables_leave_the_texture_queue` | — | 3425 | 2800 |
| `a_view_off_its_copys_elements_gets_its_own_bytes` | — | 3483 | — |
| `find` | — | — | 33 |
| `find_wrapped` | — | — | 39 |
| `find_to` | — | — | 44 |
| `mark` | — | — | 86 |
| `packed_dynamic` | — | — | 118 |
| `coalesce_remaps` | — | — | 320 |
| `stale_out` | — | — | 352 |
| `l1_packed_heads` | — | — | 358 |
| `placed_bytes` | — | — | 1136 |
| `mirror_hair` | — | — | 1427 |
| `resource_at` | — | — | 1432 |
| `slot_has_resource` | — | — | 1437 |
| `mirror_impl` | — | — | 1442 |
| `shift_levels` | — | — | 1579 |
| `shift` | — | — | 1580 |
| `shifted_levels_stay_inside_the_kept_ones` | — | — | 1610 |
| `bounded_texture_queue_matches_full_sort` | — | — | 2642 |
| `l1_packed_mip_heads_keep_neighbours_and_remap` | — | — | 2716 |
| `coalesced_remaps_find_exactly_the_same_texture_slots` | — | — | 2737 |
| `x1_dynamic_cursor_crossing_is_not_physical_wrap` | — | — | 2835 |
| `p2_packed_dynamic_matches_byte_search` | — | — | 2874 |
