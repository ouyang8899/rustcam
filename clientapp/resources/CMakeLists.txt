# Resources subdirectory - only handles Qt resources, translations, etc.
# Note: common, core, interfaces are added in the root CMakeLists.txt

set(RESOURCES_DIR ${CMAKE_CURRENT_SOURCE_DIR})

# Resource files are processed via CMAKE_AUTORCC (Qt's resource compiler)
# Place .qrc files here if needed
