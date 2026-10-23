#include "bsp/esp-bsp.h"
#include "esp_lv_adapter.h"
#include "esp_heap_caps.h"
#include "esp_timer.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "plico_p4.h"
#include <atomic>
#include <cstring>

static const char *TAG="PLICO-P4";
static std::atomic<unsigned> phase{0}; // 0: display baseline, 1: mounted, 2: smoke complete
static unsigned calls=0, frames=0;
static int64_t update_us=0, update_max_us=0;
static int64_t mount_at=0;
static std::atomic<unsigned> animation_mode{0};
#ifdef PLICO_ANIM_BENCH
#include "animation_benchmark.hpp"
#endif

static void memory(const char *name) {
    ESP_LOGI(TAG,"MEM phase=%s internal_free=%u internal_min=%u internal_largest=%u psram_free=%u psram_largest=%u",
        name,(unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT),
        (unsigned)heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT),
        (unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT),
        (unsigned)heap_caps_get_free_size(MALLOC_CAP_SPIRAM),
        (unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM));
}

// Only this LVGL callback invokes Rust. Initialization from app_main would put
// Plico's thread-local state on a different task from touch events and updates.
static void tick(lv_timer_t *) {
    auto now=esp_timer_get_time();
    static int64_t next_log=0;
    if (phase.load()==0) {
        if (now<5000000) return;
        memory("before_mount");
        auto begin=esp_timer_get_time();
        int result=plico_p4_create(BSP_LCD_H_RES,BSP_LCD_V_RES);
        ESP_LOGI(TAG,"CREATE result=%d duration_us=%lld",result,(long long)(esp_timer_get_time()-begin));
        if (result!=0) abort();
        memory("after_mount");
        mount_at=now; phase.store(1); next_log=now+2000000;
        return;
    }
    if (phase.load()==1 && now-mount_at>2000000) {
        int result=plico_p4_self_test();
        ESP_LOGI(TAG,"SELF_TEST result=%d (native CLICKED, identity reuse, conditional mount)",result);
        memory("after_self_test");
        phase.store(2);
    }
    auto begin=esp_timer_get_time();
    int result=0;
#ifdef PLICO_ANIM_BENCH
    if (bench_select(now-mount_at)==0)
#endif
        result=plico_p4_update((uint32_t)(now/1000));
    int64_t elapsed=esp_timer_get_time()-begin;
    ++calls; update_us+=elapsed; if (elapsed>update_max_us) update_max_us=elapsed;
    if (result<0) { ESP_LOGE(TAG,"UPDATE failed=%d",result); abort(); }
    if (result>0) ++frames;
    if (now>=next_log) {
#ifdef PLICO_ANIM_BENCH
        ESP_LOGI(TAG,"NATIVE mode=%u frames=%u avg_us=%lld max_us=%lld",animation_mode.load(),native_frames,
            (long long)(native_frames?native_us/native_frames:0),(long long)native_max_us);
        native_frames=0; native_us=native_max_us=0;
#endif
        ESP_LOGI(TAG,"UI calls=%u changed=%u avg_us=%lld max_us=%lld gui_stack_free=%u count=%u",
            calls,frames,(long long)(calls?update_us/calls:0),(long long)update_max_us,
            (unsigned)uxTaskGetStackHighWaterMark(nullptr),plico_p4_count());
        memory("steady");
        calls=frames=0; update_us=update_max_us=0; next_log=now+2000000;
    }
}

static void cpu(void *) {
    TaskStatus_t tasks[32];
    struct Previous { TaskHandle_t task=nullptr; uint32_t runtime=0; } old[32];
    uint32_t last_total=0;
    for (;;) {
        configRUN_TIME_COUNTER_TYPE total=0;
        unsigned count=uxTaskGetSystemState(tasks,32,&total);
        uint32_t interval=uint32_t(total)-last_total;
        if (count && last_total && interval) {
            unsigned idle[2]={0,0};
            for (unsigned i=0;i<count;++i) {
                uint32_t delta=0;
                for (auto &p:old) if (p.task==tasks[i].xHandle) { delta=uint32_t(tasks[i].ulRunTimeCounter)-p.runtime; break; }
                unsigned pct100=unsigned(uint64_t(delta)*10000/interval);
                const char *name=tasks[i].pcTaskName;
                if (!strcmp(name,"IDLE0")) idle[0]=pct100;
                if (!strcmp(name,"IDLE1")) idle[1]=pct100;
                if (strstr(name,"lvgl") || strstr(name,"lv_adapter"))
                    ESP_LOGI(TAG,"TASK phase=%u name=%s cpu_pct=%u.%02u stack_free=%u",phase.load(),name,pct100/100,pct100%100,(unsigned)tasks[i].usStackHighWaterMark);
            }
            ESP_LOGI(TAG,"CPU phase=%u mode=%u core0_busy_pct=%u.%02u core1_busy_pct=%u.%02u tasks=%u",
                phase.load(),animation_mode.load(),(10000-idle[0])/100,(10000-idle[0])%100,(10000-idle[1])/100,(10000-idle[1])%100,count);
        }
        for (unsigned i=0;i<32;++i) old[i]=i<count?Previous{tasks[i].xHandle,uint32_t(tasks[i].ulRunTimeCounter)}:Previous{};
        last_total=uint32_t(total);
        vTaskDelay(pdMS_TO_TICKS(2000));
    }
}

extern "C" void app_main() {
    bsp_display_cfg_t config={};
    config.lv_adapter_cfg=ESP_LV_ADAPTER_DEFAULT_CONFIG();
    config.lv_adapter_cfg.task_stack_size=32*1024;
    config.lv_adapter_cfg.task_priority=5;
    config.lv_adapter_cfg.task_core_id=1;
    config.lv_adapter_cfg.stack_in_psram=true;
    config.rotation=ESP_LV_ADAPTER_ROTATE_0;
    config.tear_avoid_mode=ESP_LV_ADAPTER_TEAR_AVOID_MODE_TRIPLE_PARTIAL;
    ESP_LOGI(TAG,"Starting display/touch baseline; Plico mounts after 5 seconds");
    auto display=bsp_display_start_with_config(&config);
    if (!display) abort();
    ESP_ERROR_CHECK(esp_lv_adapter_lock(-1));
    lv_timer_create(tick,20,nullptr);
    esp_lv_adapter_unlock();
    ESP_ERROR_CHECK(bsp_display_backlight_on());
    memory("display_only");
    xTaskCreate(cpu,"plico_stats",8192,nullptr,1,nullptr);
}
