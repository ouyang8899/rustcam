set(PLUGIN_ID "video_tour")

set(PLUGIN_SOURCES
    VideoTourPlugin.cpp
    TourScheme.cpp
    TourBatchPlanner.cpp
    TourApi.cpp
    TourPlayer.cpp
    TourPlayerManager.cpp
    TourScheduler.cpp
    TourChannelModel.cpp
    TourChannelStatusPanel.cpp
    TourAuditLog.cpp
    SingleInstanceGuard.cpp
    TourSchemeListPanel.cpp
    TourSchemeEditorDialog.cpp
    TourRunWidget.cpp
)

set(PLUGIN_HEADERS
    VideoTourPlugin.h
    TourTypes.h
    TourScheme.h
    TourBatchPlanner.h
    TourApi.h
    TourPlayer.h
    TourPlayerManager.h
    TourScheduler.h
    TourChannelModel.h
    TourChannelStatusPanel.h
    TourAuditLog.h
    SingleInstanceGuard.h
    TourSchemeListPanel.h
    TourSchemeEditorDialog.h
    TourRunWidget.h
)

add_library(${PLUGIN_ID} SHARED
    ${PLUGIN_SOURCES}
    ${PLUGIN_HEADERS}
    plugin.json
)

add_dependencies(${PLUGIN_ID} interfaces rustcam-core)

target_include_directories(${PLUGIN_ID} PRIVATE
    ${CMAKE_SOURCE_DIR}
    ${CMAKE_SOURCE_DIR}/core
    ${CMAKE_SOURCE_DIR}/interfaces
    ${CMAKE_SOURCE_DIR}/common
    ${CMAKE_SOURCE_DIR}/3rd_party/bridge_player/include
    ${CMAKE_SOURCE_DIR}/3rd_party/boost/include/boost-1_91
    ${CMAKE_SOURCE_DIR}/3rd_party/ffmpeg-n8.1.3-win64-lgpl-shared-8.1/include
    ${CMAKE_SOURCE_DIR}/3rd_party/jsoncpp/include
)

target_link_libraries(${PLUGIN_ID} PUBLIC
    interfaces
    common
    rustcam-core
    Qt5::Core
    Qt5::Widgets
    Qt5::Concurrent
)

set_target_properties(${PLUGIN_ID} PROPERTIES
    OUTPUT_NAME "rustcam-${PLUGIN_ID}"
    AUTOMOC ON
)

# 单元测试：仅在 Qt5 Test 可用时构建，覆盖方案编解码与批次规划纯逻辑。
# 两个测试类各自带 QTEST_MAIN，因此拆成两个可执行文件。
find_package(Qt5 COMPONENTS Test QUIET)
if(Qt5Test_FOUND)
    enable_testing()

    add_executable(video_tour_scheme_test
        tests/TourSchemeTest.cpp
        TourScheme.cpp
    )
    target_include_directories(video_tour_scheme_test PRIVATE
        ${CMAKE_CURRENT_SOURCE_DIR}
        ${CMAKE_SOURCE_DIR}
    )
    target_link_libraries(video_tour_scheme_test PRIVATE Qt5::Core Qt5::Test)
    add_test(NAME video_tour_scheme_test COMMAND video_tour_scheme_test)

    add_executable(video_tour_planner_test
        tests/TourBatchPlannerTest.cpp
        TourBatchPlanner.cpp
    )
    target_include_directories(video_tour_planner_test PRIVATE
        ${CMAKE_CURRENT_SOURCE_DIR}
        ${CMAKE_SOURCE_DIR}
    )
    target_link_libraries(video_tour_planner_test PRIVATE Qt5::Core Qt5::Test)
    add_test(NAME video_tour_planner_test COMMAND video_tour_planner_test)
endif()

add_custom_command(TARGET ${PLUGIN_ID} POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/icons/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        $<TARGET_FILE:${PLUGIN_ID}>
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CMAKE_CURRENT_SOURCE_DIR}/plugin.json
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CMAKE_CURRENT_SOURCE_DIR}/icons/icon.svg
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/icons/
)
