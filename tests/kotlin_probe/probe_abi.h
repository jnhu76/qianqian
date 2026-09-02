/*
 * probe_abi.h — single parse unit for the Kotlin/Native cinterop step.
 *
 * The probe's binding surface is exactly: the frozen application-facing C
 * ABI (player_engine.h -> songcore.h) plus the minimal CRT stdio surface
 * needed to implement the host FILE* song_io from Kotlin. One header on
 * purpose: cinterop drops declarations when unrelated headers are listed
 * separately in a .def file.
 *
 * Consumer-side only — never included by production code.
 */
#ifndef QIANQIAN_PROBE_ABI_H
#define QIANQIAN_PROBE_ABI_H

#include "player_engine.h"
#include "probe_stdio.h"

#endif /* QIANQIAN_PROBE_ABI_H */
