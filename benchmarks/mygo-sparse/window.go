// This main is overlaid onto examples/counter-native/main.go by window.sh.
package main

import (
	"context"
	"fmt"
	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"log"
	"math"
	"os"
	"runtime/pprof"
	"sync/atomic"
	"time"
)

type demo struct {
	win                      *mygo.Window
	scene, driver, frequency int
	running                  bool
	labels                   []string
	paragraphs               []string
	versions                 []string
	columns                  []ui.TableColumn
	table, list              ui.ListState
	latest                   atomic.Uint64
	config                   atomic.Uint64
	wake                     chan struct{}
	hud                      bool
}

func newDemo() *demo {
	a := &demo{running: true, frequency: 2, wake: make(chan struct{}, 1), hud: os.Getenv("SPARSE_HUD") != "0"}
	if os.Getenv("SPARSE_DRIVER") == "paint" {
		a.driver = 1
	}
	for i := 0; i < 1000; i++ {
		a.labels = append(a.labels, fmt.Sprintf("%03d:00", i))
	}
	for i := 0; i < 5000; i++ {
		a.paragraphs = append(a.paragraphs, fmt.Sprintf("Message %06d: The reply is streamed into one paragraph while older messages remain unchanged. Layout caches should preserve their shaped text and only adjust placement when needed.", i))
	}
	for i := 0; i < 4096; i++ {
		a.versions = append(a.versions, fmt.Sprintf("Streaming token %06d: The reply is streamed into one paragraph while older messages remain unchanged. Layout caches should preserve their shaped text and only adjust placement when needed.", i))
	}
	for i := 0; i < 256; i++ {
		a.columns = append(a.columns, ui.TableColumn{Title: fmt.Sprintf("Field %03d", i), Width: 78, Fixed: true})
	}
	return a
}
func (a *demo) publish() {
	config := uint64(a.frequency+1) | uint64(a.driver)<<8
	if a.running {
		config |= 1 << 16
	}
	if a.config.Swap(config) != config {
		select {
		case a.wake <- struct{}{}:
		default:
		}
	}
}
func (a *demo) worker(ctx context.Context) {
	var ticker *time.Ticker
	var ticks <-chan time.Time
	defer func() {
		if ticker != nil {
			ticker.Stop()
		}
	}()
	for {
		select {
		case <-ctx.Done():
			return
		case <-a.wake:
			if ticker != nil {
				ticker.Stop()
				ticks = nil
			}
			cfg := a.config.Load()
			if cfg&(1<<16) != 0 {
				hz := []int{10, 30, 60}[(cfg&255)-1]
				ticker = time.NewTicker(time.Second / time.Duration(hz))
				ticks = ticker.C
			}
		case <-ticks:
			a.latest.Add(1)
			if a.config.Load()&(1<<8) == 0 {
				a.win.Invalidate()
			} // Built-in coalescing, no Update queue.
		}
	}
}
func (a *demo) liveText(c *ui.Context, label string, width, height float32) {
	if a.driver == 0 {
		ui.Text(c, label).FontSize(13).FixedLineHeight(height).Width(width).Height(height).SingleLine().TextColor(c.Theme().Accent)
	} else {
		color := c.Theme().Accent
		ui.Box(c).Size(width, height).Clip().Draw(func(p *ui.Painter, r ui.Rect) {
			var value string
			if a.scene == 0 {
				value = fmt.Sprintf("000:%02d", a.latest.Load()%100)
			} else {
				value = fmt.Sprintf("Batch %06d", a.latest.Load()%1000000)
			}
			p.Text(r.X, r.Y, value, 13, color)
			if a.config.Load()&(1<<16) != 0 {
				p.After(time.Second / time.Duration([]int{10, 30, 60}[int(a.config.Load()&255)-1]))
			}
		})
	}
}
func (a *demo) view(c *ui.Context, stats *ui.SparseWindowStats) {
	t := c.Theme()
	ui.Column(c).Fill().Padding(14).Gap(10).Children(func() {
		ui.Text(c, "MyGO sparse-update lab").FontSize(24).Bold()
		ui.Segmented(c, &a.scene, "1000 visible texts", "Wide table (256 cols)", "5000 paragraphs / Scroll", "5000 paragraphs / List").Label("Scenario")
		ui.Row(c).Gap(14).AlignItems(ui.Center).Children(func() {
			ui.Checkbox(c, &a.running, "Running")
			ui.Segmented(c, &a.driver, "State / Text", "Paint-only control").Label("Driver")
			ui.Segmented(c, &a.frequency, "10 Hz", "30 Hz", "60 Hz").Label("Update rate")
			if ui.Button(c, "Step").Clicked() {
				a.latest.Add(1)
			}
		})
		if a.hud {
			ui.Box(c).Height(70).Shrink(0).Radius(8).Background(t.Surface).Clip().Draw(func(p *ui.Painter, r ui.Rect) {
				for i, line := range stats.Lines {
					p.Text(r.X+10, r.Y+7+float32(i)*19, line, 13, t.Text)
				}
			})
		}
		caption := "All 1000 labels are visible; only the first label changes. Cached layouts stay enabled."
		if a.scene == 1 {
			caption = "Native Table, row virtualization enabled; one status label changes. Horizontal columns are not virtualized."
		}
		if a.scene >= 2 {
			caption = "Only the first paragraph changes. Scroll retains all 5000; List builds the visible rows. This is a cache/virtualization stress test."
		}
		if a.driver == 1 {
			caption = "Control uses custom drawing for the changing content and skips view rebuilds. It is NOT a reactive implementation."
		}
		ui.Text(c, caption).FontSize(13).TextColor(t.TextMuted).SingleLine()
		switch a.scene {
		case 0:
			_, h := c.Size()
			rowH := float32(math.Max(14, float64((h-280)/40)))
			ui.Column(c).Grow(1).Gap(0).Children(func() {
				for row := 0; row < 40; row++ {
					ui.Row(c).Height(rowH).Shrink(0).Gap(3).Children(func() {
						for col := 0; col < 25; col++ {
							i := row*25 + col
							ui.Column(c).Width(0).Grow(1).Height(rowH).Children(func() {
								if i == 0 {
									a.liveText(c, fmt.Sprintf("000:%02d", a.latest.Load()%100), 52, rowH)
								} else {
									ui.Text(c, a.labels[i]).FontSize(13).FixedLineHeight(rowH).Height(rowH).SingleLine()
								}
							})
						}
					})
				}
			})
		case 1:
			a.liveText(c, fmt.Sprintf("Batch %06d", a.latest.Load()%1000000), 160, 22)
			ui.Table(c, &a.table, a.columns, 100000, func(row, col int) { ui.Text(c, a.labels[(row*7+col)%len(a.labels)]).FontSize(13).SingleLine() }).Grow(1)
		default:
			if a.driver == 1 {
				a.liveText(c, "", 160, 22)
			}
			paragraph := func(i int) {
				content := a.paragraphs[i]
				if i == 0 && a.driver == 0 {
					content = a.versions[a.latest.Load()%uint64(len(a.versions))]
				}
				ui.Text(c, content).FontSize(14)
			}
			if a.scene == 2 {
				ui.Scroll(c).Grow(1).Padding(12).Gap(12).Children(func() {
					for i := range a.paragraphs {
						paragraph(i)
					}
				})
			} else {
				ui.List(c, &a.list, len(a.paragraphs), paragraph).Grow(1).Padding(12).Gap(12)
			}
		}
	})
	a.publish()
}
func main() {
	a := newDemo()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	profileDone := make(chan struct{})
	go func() { defer close(profileDone); recordProfile(ctx) }()
	defer func() { cancel(); <-profileDone }()
	mygo.App.WhenReady(func() {
		a.win = mygo.NewWindow(mygo.WindowOptions{Title: "MyGO sparse-update lab", Width: 1440, Height: 1060, MinWidth: 1180, MinHeight: 1000, UseContentSize: true, Content: ui.SparseDiagnosticView(a.view)})
		a.win.OnClosed(cancel)
		a.publish()
		go a.worker(ctx)
	})
	if err := mygo.App.Run(); err != nil {
		log.Fatal(err)
	}
}

// Optional real-window profile: settle for 2s, sample for 6s. No work is added
// to the UI loop; SPARSE_AUTOCLOSE only closes explicitly requested test runs.
func recordProfile(ctx context.Context) {
	path := os.Getenv("SPARSE_PROFILE")
	if path == "" {
		return
	}
	select {
	case <-ctx.Done():
		return
	case <-time.After(2 * time.Second):
	}
	f, err := os.Create(path)
	if err != nil {
		log.Printf("profile: %v", err)
		return
	}
	if err := pprof.StartCPUProfile(f); err != nil {
		f.Close()
		log.Printf("profile: %v", err)
		return
	}
	select {
	case <-ctx.Done():
	case <-time.After(6 * time.Second):
	}
	pprof.StopCPUProfile()
	f.Close()
	if os.Getenv("SPARSE_AUTOCLOSE") == "1" {
		mygo.App.Quit()
	}
}
