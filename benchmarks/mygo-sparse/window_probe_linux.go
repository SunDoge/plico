//go:build linux

// Injected diagnostic adapter only. The normal MyGO checkout is unchanged.
package ui

import (
	"fmt"
	"github.com/egoist/mygo/internal/scene"
	"log"
	"os"
	"runtime"
	"strconv"
	"syscall"
	"time"
)

// SparseWindowStats describes completed frames, never the frame being built.
type SparseWindowStats struct {
	Lines          [3]string
	frameCount     int
	epoch          time.Time
	cpu            time.Duration
	cpuOne, cpuAll float64
	hz             float64
}

func sparseProcessCPU() time.Duration {
	var r syscall.Rusage
	if syscall.Getrusage(syscall.RUSAGE_SELF, &r) != nil {
		return 0
	}
	return time.Duration(r.Utime.Nano() + r.Stime.Nano())
}

// SparseDiagnosticView uses the real native window host and renderer. Its sole
// addition is timing around present; it never skips a node or changes scheduling.
func SparseDiagnosticView(view func(*Context, *SparseWindowStats)) *Content {
	return coreView(func(c *context) {
		h, ok := c.rt.host.(*sparseWindowHost)
		if !ok {
			h = &sparseWindowHost{host: c.rt.host, rt: c.rt, cores: runtime.NumCPU()}
			if n, err := strconv.Atoi(os.Getenv("HOST_CPUS")); err == nil && n > 0 {
				h.cores = n
			}
			h.stats.epoch, h.stats.cpu = time.Now(), sparseProcessCPU()
			h.stats.Lines[0] = "Warming up native renderer and text caches..."
			c.rt.host = h
			if c.rt.stats == nil {
				c.rt.stats = newFrameStats(time.Hour)
				c.rt.stats.start, c.rt.stats.last = time.Now(), time.Now()
				c.rt.stats.layouts = c.rt.text.LayoutsMade()
			}
		}
		view(makeContext(c), &h.stats)
	})
}

type sparseWindowHost struct {
	host
	rt      *engine
	cores   int
	stats   SparseWindowStats
	lastHUD time.Time
}

func (h *sparseWindowHost) framePath() string {
	if p, ok := h.host.(interface{ framePath() string }); ok {
		return p.framePath()
	}
	return "native host"
}
func (h *sparseWindowHost) present(s *scene.Scene) {
	prepared := time.Now()
	f := h.rt.stats
	ready := prepared.Sub(f.start)
	h.host.present(s) // The real GPU/native present remains intact.
	now := time.Now()
	present := now.Sub(prepared)
	h.stats.frameCount++
	if span := now.Sub(h.stats.epoch); span >= time.Second {
		cpu := sparseProcessCPU()
		h.stats.cpuOne = (cpu - h.stats.cpu).Seconds() / span.Seconds() * 100
		h.stats.cpuAll = h.stats.cpuOne / float64(h.cores)
		h.stats.hz = float64(h.stats.frameCount) / span.Seconds()
		h.stats.epoch, h.stats.cpu, h.stats.frameCount = now, cpu, 0
	}
	// Format the HUD only 4 times/sec. Its Draw closure can refresh the labels
	// during repaint-only frames without asking for a fresh declaration.
	if now.Sub(h.lastHUD) < 250*time.Millisecond {
		return
	}
	h.lastHUD = now
	ms := func(d time.Duration) float64 { return d.Seconds() * 1000 }
	h.stats.Lines[0] = fmt.Sprintf("CPU prepare %.2f ms   |   build %.2f   layout %.2f   paint %.2f   |   present %.2f ms",
		ms(ready), ms(f.laps[phaseBuild]), ms(f.laps[phaseLayout]), ms(f.laps[phasePaint]), ms(present))
	h.stats.Lines[1] = fmt.Sprintf("Completed %.1f frames/s   |   process CPU %.1f%% of one core / %.2f%% of %d logical CPUs   |   nodes %d",
		h.stats.hz, h.stats.cpuOne, h.stats.cpuAll, h.cores, h.rt.c.used)
	h.stats.Lines[2] = fmt.Sprintf("New text layouts: %d   |   %s", h.rt.text.LayoutsMade()-f.layouts, h.framePath())
	if os.Getenv("SPARSE_LOG") == "1" {
		log.Printf("sparse-window: %s; %s; %s", h.stats.Lines[0], h.stats.Lines[1], h.stats.Lines[2])
	}
}
