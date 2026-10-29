//go:build linux

// This file is injected into MyGO/ui with go test -overlay; no MyGO source is edited.
package ui

import (
	"fmt"
	"os"
	"slices"
	"strconv"
	"syscall"
	"testing"
	"time"

	"github.com/egoist/mygo/internal/platform"
	"github.com/egoist/mygo/internal/scene"
)

// Keep the shipping CPU declaration/layout/scene path; omit rasterization and GPU.
type sparseProbeHost struct {
	headless
	presents int
}

func (h *sparseProbeHost) present(s *scene.Scene) { h.last = s; h.presents++ }

type sparseProbe struct {
	rt                      *engine
	host                    *sparseProbeHost
	list                    ListState
	columns                 []TableColumn
	rows                    int
	mode                    string
	value                   string
	labels                  []string
	clock                   time.Time
	declared                time.Duration
	builds, cells, drawings int
	steps                   int
}

func newSparseProbe(rows, cols int, mode string) *sparseProbe {
	p := &sparseProbe{rows: rows, mode: mode, value: "101.24", clock: time.Unix(1000, 0)}
	for i := 0; i < cols; i++ {
		p.columns = append(p.columns, TableColumn{Title: fmt.Sprintf("Metric %02d", i), Width: 78, Fixed: true})
	}
	// All strings are prepared outside the update loop, including both quote values.
	for i := 0; i < 256; i++ {
		p.labels = append(p.labels, fmt.Sprintf("%03d.%02d", i, i%100))
	}
	p.host = &sparseProbeHost{headless: headless{w: 1280, h: 900, scale: 1, hz: 60}}
	p.rt = newRuntime(func(raw *context) {
		start := time.Now()
		defer func() { p.declared += time.Since(start); p.builds++ }()
		c := makeContext(raw)
		Column(c).Fill().Padding(8).Gap(8).Children(func() {
			Row(c).Height(32).Shrink(0).Gap(12).Children(func() {
				Text(c, "Query results").SingleLine()
				if mode == "host-status" {
					Text(c, p.value).SingleLine()
				} else {
					Text(c, "Connected").SingleLine()
				}
				if mode == "paint-animation" {
					Box(c).Size(100, 16).Draw(func(painter *Painter, r Rect) {
						p.drawings++
						painter.AnimationFrame()
						painter.Fill(Rect{r.X, r.Y, float32(painter.Now().UnixMilli() % 100), r.H}, RGB(50, 120, 220), 2)
					})
				}
			})
			Table(c, &p.list, p.columns, rows, func(row, col int) {
				p.cells++
				label := p.labels[(row*7+col)%len(p.labels)]
				if mode == "host-cell" && row == 0 && col == 0 {
					label = p.value
				}
				Text(c, label).FontSize(13).SingleLine()
			}).Grow(1)
		})
	}, p.host)
	p.rt.clock = func() time.Time { return p.clock }
	p.rt.stats = newFrameStats(time.Hour) // Record phase laps without formatting/logging.
	for i := 0; i < 30; i++ {
		p.rt.changed() // Settle the same row window before enabling paint-only frames.
		p.step()
	}
	return p
}
func (p *sparseProbe) step() {
	p.clock = p.clock.Add(time.Second / 60)
	if p.value == "101.24" {
		p.value = "101.25"
	} else {
		p.value = "101.24"
	}
	p.steps++
	if p.mode == "scroll" {
		delta := 8.
		if p.steps%240 >= 120 {
			delta = -delta
		}
		p.rt.event(platform.SurfaceEvent{Kind: platform.PointerScroll, X: 500, Y: 400, DY: delta, Precise: true})
	} else if p.mode != "paint-animation" {
		p.rt.changed()
	}
	p.rt.surfaceFrame()
}
func BenchmarkPlicoSparseTable(b *testing.B) {
	for _, cols := range []int{4, 12, 24, 64, 128, 256, 512} {
		for _, rows := range []int{10000, 100000} {
			for _, mode := range []string{"host-cell", "host-status", "paint-animation", "scroll"} {
				b.Run(fmt.Sprintf("cols=%d/rows=%d/%s", cols, rows, mode), func(b *testing.B) {
					p := newSparseProbe(rows, cols, mode)
					defer p.rt.close()
					p.declared, p.builds, p.cells = 0, 0, 0
					var phases [phases]time.Duration
					var passes, shaped uint64
					samples := make([]time.Duration, 0, 100000)
					var missed60, missed120 int
					b.ReportAllocs()
					b.ResetTimer()
					for b.Loop() {
						before := p.rt.text.LayoutsMade()
						start := time.Now()
						p.step()
						elapsed := time.Since(start)
						if len(samples) < cap(samples) {
							samples = append(samples, elapsed)
						}
						if elapsed > time.Second/60 {
							missed60++
						}
						if elapsed > time.Second/120 {
							missed120++
						}
						shaped += p.rt.text.LayoutsMade() - before
						for phase, elapsed := range p.rt.stats.laps {
							phases[phase] += elapsed
						}
						passes += uint64(p.rt.stats.passes)
					}
					slices.Sort(samples)
					b.ReportMetric(float64(samples[len(samples)/2].Nanoseconds()), "p50-ns")
					b.ReportMetric(float64(samples[len(samples)*95/100].Nanoseconds()), "p95-ns")
					b.ReportMetric(float64(samples[len(samples)*99/100].Nanoseconds()), "p99-ns")
					b.ReportMetric(float64(missed60)*100/float64(b.N), "over60-pct")
					b.ReportMetric(float64(missed120)*100/float64(b.N), "over120-pct")
					b.ReportMetric(float64(p.declared.Nanoseconds())/float64(b.N), "declare-ns/op")
					for phase, name := range []string{"build-ns/op", "layout-ns/op", "paint-ns/op", "present-ns/op"} {
						b.ReportMetric(float64(phases[phase].Nanoseconds())/float64(b.N), name)
					}
					b.ReportMetric(float64(p.rt.c.used), "nodes")
					b.ReportMetric(float64(p.cells)/float64(b.N), "cells/op")
					b.ReportMetric(float64(passes)/float64(b.N), "passes/op")
					b.ReportMetric(float64(shaped)/float64(b.N), "text-layouts/op")
				})
			}
		}
	}
}
func TestPlicoSparseTable(t *testing.T) {
	for _, mode := range []string{"host-cell", "host-status", "paint-animation"} {
		p := newSparseProbe(100000, 12, mode)
		p.builds, p.cells, p.drawings = 0, 0, 0
		p.step()
		if mode == "paint-animation" {
			if p.builds != 0 || p.cells != 0 || p.drawings == 0 {
				t.Fatalf("paint-only fast path: builds=%d cells=%d draws=%d", p.builds, p.cells, p.drawings)
			}
		} else {
			if p.builds != 1 || p.cells == 0 || p.cells > 12*50 {
				t.Fatalf("expected one virtualized build, builds=%d cells=%d", p.builds, p.cells)
			}
			found := false
			for i := 0; i < p.rt.c.used; i++ {
				n := &p.rt.c.chunks[i/chunkSize][i%chunkSize]
				if n.text == p.value {
					found = true
				}
			}
			if !found {
				t.Fatal("changed text did not reach the current UI tree")
			}
		}
		p.rt.close()
	}
}

// TestPlicoSparseCPU is opt-in: the timed interval excludes compilation and warmup.
// It measures process user+system CPU, including native text calls and Go GC.
func TestPlicoSparseCPU(t *testing.T) {
	if os.Getenv("MYGO_SPARSE_CPU") != "1" {
		t.Skip("set MYGO_SPARSE_CPU=1 for paced process CPU measurement")
	}
	duration := 3 * time.Second
	if value := os.Getenv("LOAD_TIME"); value != "" {
		parsed, err := time.ParseDuration(value)
		if err != nil || parsed < time.Second {
			t.Fatal("LOAD_TIME must be at least 1s")
		}
		duration = parsed
	}
	cores, err := strconv.Atoi(os.Getenv("HOST_CPUS"))
	if err != nil || cores < 1 {
		t.Fatal("HOST_CPUS must name the host logical CPU count")
	}
	cpuTime := func() time.Duration {
		var usage syscall.Rusage
		if err := syscall.Getrusage(syscall.RUSAGE_SELF, &usage); err != nil {
			t.Fatal(err)
		}
		return time.Duration(usage.Utime.Nano() + usage.Stime.Nano())
	}
	for _, cols := range []int{12, 64, 128, 256, 512} {
		for _, mode := range []string{"host-status", "paint-animation"} {
			p := newSparseProbe(100000, cols, mode)
			samples := make([]time.Duration, 0, int(duration.Seconds()*120))
			layouts := p.rt.text.LayoutsMade()
			frames, missed := 0, 0
			var work time.Duration
			ticker := time.NewTicker(time.Second / 60)
			cpuStart := cpuTime()
			start := time.Now()
			for time.Since(start) < duration {
				<-ticker.C // Slow frames naturally coalesce ticks; no growing event queue.
				frameStart := time.Now()
				p.step()
				elapsed := time.Since(frameStart)
				work += elapsed
				samples = append(samples, elapsed)
				frames++
				if elapsed > time.Second/60 {
					missed++
				}
			}
			wall, cpu := time.Since(start), cpuTime()-cpuStart
			ticker.Stop()
			slices.Sort(samples)
			oneCore := cpu.Seconds() / wall.Seconds() * 100
			fmt.Printf("LOAD mode=%s cols=%d rows=100000 requested_hz=60 frames=%d cpu_onecore_pct=%.2f cpu_all_%dcores_pct=%.3f prepare_fps=%.2f mean_ms=%.3f p95_ms=%.3f over60_pct=%.2f text_layouts=%d\n",
				mode, cols, frames, oneCore, cores, oneCore/float64(cores), float64(frames)/wall.Seconds(),
				work.Seconds()*1000/float64(frames), float64(samples[len(samples)*95/100])/float64(time.Millisecond), float64(missed)*100/float64(frames), p.rt.text.LayoutsMade()-layouts)
			p.rt.close()
		}
	}
}

// Paragraph probe: only the first visible paragraph changes. All other strings,
// widths and typography stay unchanged. Compare Scroll with the existing List.
var sparseTextSerial int

func newParagraphProbe(count int, virtual, changing bool) *sparseProbe {
	serial := sparseTextSerial
	sparseTextSerial++
	p := &sparseProbe{rows: count, mode: "paragraph", clock: time.Unix(1000, 0)}
	body := "The reply is streamed into one paragraph while older messages remain unchanged. Layout caches should preserve their shaped text and only adjust placement when needed. "
	for i := 0; i < count; i++ {
		p.labels = append(p.labels, fmt.Sprintf("Message %06d: %s", i, body))
	}
	versions := make([]string, 4096)
	for i := range versions {
		versions[i] = fmt.Sprintf("Stream %06d token %06d: %s", serial, i, body)
	}
	p.host = &sparseProbeHost{headless: headless{w: 760, h: 900, scale: 1, hz: 60}}
	p.rt = newRuntime(func(raw *context) {
		start := time.Now()
		defer func() { p.declared += time.Since(start); p.builds++ }()
		c := makeContext(raw)
		Column(c).Fill().Children(func() {
			Text(c, "Conversation transcript").Height(32)
			paragraph := func(i int) {
				p.cells++
				content := p.labels[i]
				if i == 0 {
					if changing {
						content = versions[p.steps%len(versions)]
					} else {
						content = versions[0]
					}
				}
				Text(c, content).FontSize(14)
			}
			if virtual {
				List(c, &p.list, count, paragraph).Grow(1).Padding(12).Gap(12)
			} else {
				Scroll(c).Grow(1).Padding(12).Gap(12).Children(func() {
					for i := range p.labels {
						paragraph(i)
					}
				})
			}
		})
	}, p.host)
	p.rt.clock = func() time.Time { return p.clock }
	p.rt.stats = newFrameStats(time.Hour)
	for i := 0; i < 6; i++ {
		p.step()
	}
	return p
}
func BenchmarkPlicoSparseParagraphs(b *testing.B) {
	for _, count := range []int{64, 512, 2048, 5000} {
		for _, virtual := range []bool{false, true} {
			for _, changing := range []bool{true, false} {
				b.Run(fmt.Sprintf("paragraphs=%d/virtual=%t/changing=%t", count, virtual, changing), func(b *testing.B) {
					p := newParagraphProbe(count, virtual, changing)
					defer p.rt.close()
					p.cells, p.declared = 0, 0
					var shaped uint64
					var elapsed [phases]time.Duration
					b.ReportAllocs()
					b.ResetTimer()
					for b.Loop() {
						before := p.rt.text.LayoutsMade()
						p.step()
						shaped += p.rt.text.LayoutsMade() - before
						for i, lap := range p.rt.stats.laps {
							elapsed[i] += lap
						}
					}
					b.ReportMetric(float64(p.cells)/float64(b.N), "paragraphs/op")
					b.ReportMetric(float64(shaped)/float64(b.N), "text-layouts/op")
					b.ReportMetric(float64(p.declared.Nanoseconds())/float64(b.N), "declare-ns/op")
					b.ReportMetric(float64(elapsed[phaseLayout].Nanoseconds())/float64(b.N), "layout-ns/op")
					b.ReportMetric(float64(elapsed[phasePaint].Nanoseconds())/float64(b.N), "paint-ns/op")
				})
			}
		}
	}
}
func TestPlicoSparseParagraphs(t *testing.T) {
	for _, virtual := range []bool{false, true} {
		p := newParagraphProbe(64, virtual, true)
		p.cells = 0
		before := p.rt.text.LayoutsMade()
		p.step()
		if p.rt.text.LayoutsMade()-before > 16 {
			t.Fatal("small document unexpectedly reshaped unchanged paragraphs")
		}
		if virtual && p.cells >= 64 {
			t.Fatal("List did not virtualize the paragraph history")
		}
		p.rt.close()
	}
}
