#pragma once

#ifdef COMMON_EXPORTS
#define COMMON_API Q_DECL_EXPORT
#else
#define COMMON_API Q_DECL_IMPORT
#endif
