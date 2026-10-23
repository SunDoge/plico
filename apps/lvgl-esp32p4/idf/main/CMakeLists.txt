idf_component_register(SRCS "main.cpp" INCLUDE_DIRS "../.."
    REQUIRES esp32_p4_wifi6_touch_lcd_4_3 esp_timer esp_psram pthread vfs)
add_prebuilt_library(plico_rust "$ENV{PLICO_P4_ARCHIVE}"
    REQUIRES lvgl__lvgl newlib cxx pthread freertos esp_system)
target_link_libraries(${COMPONENT_LIB} PUBLIC plico_rust)
if("$ENV{PLICO_P4_ANIM_BENCH}" STREQUAL "1")
    target_compile_definitions(${COMPONENT_LIB} PRIVATE PLICO_ANIM_BENCH=1)
endif()
