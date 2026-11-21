#!/bin/sh
# Builds a static single-node High Performance Linpack (xhpl) executable linked against OpenBLAS
# (DYNAMIC_ARCH=1, OpenMP) and a minimal single-rank MPI library for the CPU benchmark item from
# sources pinned by SHA-256. It runs inside Alpine 3.22 on the architecture it builds for; the
# tools workflow (.github/workflows/tools.yml) starts it and publishes what it writes as RELEASE.
#
# Usage: build-hpl.sh <output dir>
# Writes hpl-<arch>, stripped, this script, the HPL and OpenBLAS source archives, the single-rank
# MPI shim sources, and the license texts of HPL, OpenBLAS and musl to the output directory. The
# binary must be static and pass HPL's residual check on a test matrix.
set -eu

RELEASE=hpl-2.3-2
# The image the tools workflow runs this script in: Alpine 3.22 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/alpine:3.22
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
HPL_VERSION=2.3
HPL_URL=https://netlib.org/benchmark/hpl/hpl-$HPL_VERSION.tar.gz
HPL_SHA256=32c5c17d22330e6f2337b681aded51637fb6008d3f0eb7c277b163fadd612830
OPENBLAS_VERSION=0.3.34
OPENBLAS_ARCHIVE=OpenBLAS-$OPENBLAS_VERSION.tar.gz
OPENBLAS_URL=https://github.com/OpenMathLib/OpenBLAS/releases/download/v$OPENBLAS_VERSION/$OPENBLAS_ARCHIVE
OPENBLAS_SHA256=cd7e129868320cc2d033afa920e31202dfe0b8066a5b66661900ccc0f197dfed
# Only for its COPYRIGHT: Alpine links musl statically but ships no copy of the notice.
MUSL_VERSION=1.2.5
MUSL_URL=https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4

fail() {
    echo "build-hpl: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

pack() { # <tarball> <path>...
    tarball=$1
    shift
    tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -cf - "$@" | gzip -n -9 \
        >"$tarball"
}

[ $# -eq 1 ] || fail "usage: build-hpl.sh <output dir>"
case $RELEASE in
"hpl-$HPL_VERSION-"*) ;;
*) fail "the release $RELEASE is not of HPL $HPL_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
work=/build
mkdir "$work" || fail "$work exists already"

apk add --no-cache binutils curl gcc gzip linux-headers make musl-dev perl tar >/dev/null
musl=$(apk list --installed musl)
case $musl in
"musl-$MUSL_VERSION-"*) ;;
*) fail "the image has $musl, but the notice is pinned for musl $MUSL_VERSION" ;;
esac
cd "$work"
fetch "$HPL_URL" "$HPL_SHA256" "hpl-$HPL_VERSION.tar.gz"
fetch "$OPENBLAS_URL" "$OPENBLAS_SHA256" "$OPENBLAS_ARCHIVE"
fetch "$MUSL_URL" "$MUSL_SHA256" "musl-$MUSL_VERSION.tar.gz"
tar -xzf "hpl-$HPL_VERSION.tar.gz"
tar -xzf "$OPENBLAS_ARCHIVE"
tar -xzf "musl-$MUSL_VERSION.tar.gz" "musl-$MUSL_VERSION/COPYRIGHT"

# 1. Build static CBLAS-only OpenBLAS with runtime microarchitecture dispatch (DYNAMIC_ARCH=1).
# Pin TARGET to the baseline core of $arch so non-kernel files (such as driver/others/memory.c)
# do not inherit the build runner's ISA (for example ARMv8.4 LRCPC2 stlur on Neoverse runners).
case $arch in
x86_64) openblas_target=PRESCOTT ;;
aarch64) openblas_target=ARMV8 ;;
*) fail "unsupported architecture: $arch" ;;
esac
make -C "$work/OpenBLAS-$OPENBLAS_VERSION" -j"$(nproc)" \
    TARGET="$openblas_target" \
    NO_SHARED=1 \
    NO_LAPACK=1 \
    NO_LAPACKE=1 \
    ONLY_CBLAS=1 \
    USE_OPENMP=1 \
    DYNAMIC_ARCH=1 \
    NUM_THREADS=256 \
    libs >/dev/null
make -C "$work/OpenBLAS-$OPENBLAS_VERSION" \
    TARGET="$openblas_target" \
    NO_SHARED=1 \
    NO_LAPACK=1 \
    NO_LAPACKE=1 \
    ONLY_CBLAS=1 \
    USE_OPENMP=1 \
    DYNAMIC_ARCH=1 \
    NUM_THREADS=256 \
    PREFIX="$work/openblas" \
    install >/dev/null

# 2. Build a minimal single-rank (P=1, Q=1) MPI library so xhpl runs directly without mpirun.
mkdir "$work/mpi-stub"
cat >"$work/mpi-stub/mpi.h" <<'EOF'
#ifndef LOWENDAFF_MPI_STUB_H
#define LOWENDAFF_MPI_STUB_H

#include <stddef.h>
#include <stdint.h>

typedef int MPI_Comm;
typedef int MPI_Datatype;
typedef int MPI_Request;
typedef ptrdiff_t MPI_Aint;

typedef struct {
    int MPI_SOURCE;
    int MPI_TAG;
    int MPI_ERROR;
    size_t bytes;
} MPI_Status;

#define MPI_SUCCESS 0
#define MPI_COMM_NULL 0
#define MPI_COMM_WORLD 1
#define MPI_UNDEFINED (-32766)

#define MPI_BYTE 1
#define MPI_INT 2
#define MPI_DOUBLE 3

int MPI_Init(int *argc, char ***argv);
int MPI_Finalize(void);
int MPI_Abort(MPI_Comm comm, int errorcode);
double MPI_Wtime(void);

int MPI_Comm_rank(MPI_Comm comm, int *rank);
int MPI_Comm_size(MPI_Comm comm, int *size);
int MPI_Comm_split(MPI_Comm comm, int color, int key, MPI_Comm *newcomm);
int MPI_Comm_free(MPI_Comm *comm);

int MPI_Get_address(const void *location, MPI_Aint *address);
int MPI_Address(void *location, MPI_Aint *address);
int MPI_Type_contiguous(int count, MPI_Datatype oldtype, MPI_Datatype *newtype);
int MPI_Type_vector(int count, int blocklength, int stride, MPI_Datatype oldtype, MPI_Datatype *newtype);
int MPI_Type_create_struct(int count, const int blocklengths[], const MPI_Aint displacements[], const MPI_Datatype types[], MPI_Datatype *newtype);
int MPI_Type_struct(int count, int blocklengths[], MPI_Aint displacements[], MPI_Datatype types[], MPI_Datatype *newtype);
int MPI_Type_commit(MPI_Datatype *datatype);
int MPI_Type_free(MPI_Datatype *datatype);

int MPI_Send(const void *buf, int count, MPI_Datatype datatype, int dest, int tag, MPI_Comm comm);
int MPI_Ssend(const void *buf, int count, MPI_Datatype datatype, int dest, int tag, MPI_Comm comm);
int MPI_Recv(void *buf, int count, MPI_Datatype datatype, int source, int tag, MPI_Comm comm, MPI_Status *status);
int MPI_Sendrecv(const void *sendbuf, int sendcount, MPI_Datatype sendtype, int dest, int sendtag, void *recvbuf, int recvcount, MPI_Datatype recvtype, int source, int recvtag, MPI_Comm comm, MPI_Status *status);
int MPI_Issend(const void *buf, int count, MPI_Datatype datatype, int dest, int tag, MPI_Comm comm, MPI_Request *request);
int MPI_Irecv(void *buf, int count, MPI_Datatype datatype, int source, int tag, MPI_Comm comm, MPI_Request *request);
int MPI_Wait(MPI_Request *request, MPI_Status *status);
int MPI_Iprobe(int source, int tag, MPI_Comm comm, int *flag, MPI_Status *status);

#endif
EOF

cat >"$work/mpi-stub/mpi_serial.c" <<'EOF'
/* Single-rank (1x1 process grid) MPI implementation for single-node HPL 2.3.
 * Licensed under the MIT license (part of lowendaff-bench).
 */
#include "mpi.h"
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define MAX_TYPES 256
#define MAX_MSGS 64

typedef struct {
    int kind; /* 0=unused, 1=contig, 2=vector, 3=struct */
    size_t extent;
} TypeDesc;

typedef struct {
    int active;
    int tag;
    const void *buf;
    int count;
    MPI_Datatype dtype;
} PendingSend;

static TypeDesc g_types[MAX_TYPES] = {
    {0, 0},
    {1, 1},              /* MPI_BYTE */
    {1, sizeof(int)},    /* MPI_INT */
    {1, sizeof(double)}, /* MPI_DOUBLE */
};

static PendingSend g_sends[MAX_MSGS];

static size_t type_size(MPI_Datatype dt) {
    if (dt > 0 && dt < MAX_TYPES && g_types[dt].kind != 0) {
        return g_types[dt].extent;
    }
    return 1;
}

static MPI_Datatype alloc_type(size_t extent) {
    for (int i = 4; i < MAX_TYPES; i++) {
        if (g_types[i].kind == 0) {
            g_types[i].kind = 1;
            g_types[i].extent = extent;
            return i;
        }
    }
    abort();
}

int MPI_Init(int *argc, char ***argv) {
    (void)argc;
    (void)argv;
    return MPI_SUCCESS;
}

int MPI_Finalize(void) { return MPI_SUCCESS; }

int MPI_Abort(MPI_Comm comm, int errorcode) {
    (void)comm;
    exit(errorcode ? errorcode : 1);
}

double MPI_Wtime(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec * 1e-9;
}

int MPI_Comm_rank(MPI_Comm comm, int *rank) {
    (void)comm;
    *rank = 0;
    return MPI_SUCCESS;
}

int MPI_Comm_size(MPI_Comm comm, int *size) {
    (void)comm;
    *size = 1;
    return MPI_SUCCESS;
}

int MPI_Comm_split(MPI_Comm comm, int color, int key, MPI_Comm *newcomm) {
    (void)comm;
    (void)key;
    *newcomm = (color == MPI_UNDEFINED) ? MPI_COMM_NULL : MPI_COMM_WORLD;
    return MPI_SUCCESS;
}

int MPI_Comm_free(MPI_Comm *comm) {
    *comm = MPI_COMM_NULL;
    return MPI_SUCCESS;
}

int MPI_Get_address(const void *location, MPI_Aint *address) {
    *address = (MPI_Aint)(uintptr_t)location;
    return MPI_SUCCESS;
}

int MPI_Address(void *location, MPI_Aint *address) {
    return MPI_Get_address(location, address);
}

int MPI_Type_contiguous(int count, MPI_Datatype oldtype, MPI_Datatype *newtype) {
    *newtype = alloc_type((size_t)count * type_size(oldtype));
    return MPI_SUCCESS;
}

int MPI_Type_vector(int count, int blocklength, int stride, MPI_Datatype oldtype, MPI_Datatype *newtype) {
    size_t elem = type_size(oldtype);
    size_t span = (count > 0) ? ((size_t)(count - 1) * (size_t)stride + (size_t)blocklength) * elem : 0;
    *newtype = alloc_type(span);
    return MPI_SUCCESS;
}

int MPI_Type_create_struct(int count, const int blocklengths[], const MPI_Aint displacements[], const MPI_Datatype types[], MPI_Datatype *newtype) {
    MPI_Aint hi = 0;
    for (int i = 0; i < count; i++) {
        MPI_Aint end = displacements[i] + (MPI_Aint)((size_t)blocklengths[i] * type_size(types[i]));
        if (end > hi) hi = end;
    }
    *newtype = alloc_type((size_t)hi);
    return MPI_SUCCESS;
}

int MPI_Type_struct(int count, int blocklengths[], MPI_Aint displacements[], MPI_Datatype types[], MPI_Datatype *newtype) {
    return MPI_Type_create_struct(count, blocklengths, displacements, types, newtype);
}

int MPI_Type_commit(MPI_Datatype *datatype) {
    (void)datatype;
    return MPI_SUCCESS;
}

int MPI_Type_free(MPI_Datatype *datatype) {
    if (*datatype >= 4 && *datatype < MAX_TYPES) {
        g_types[*datatype].kind = 0;
        g_types[*datatype].extent = 0;
    }
    *datatype = 0;
    return MPI_SUCCESS;
}

int MPI_Issend(const void *buf, int count, MPI_Datatype datatype, int dest, int tag, MPI_Comm comm, MPI_Request *request) {
    (void)dest;
    (void)comm;
    for (int i = 0; i < MAX_MSGS; i++) {
        if (!g_sends[i].active) {
            g_sends[i].active = 1;
            g_sends[i].tag = tag;
            g_sends[i].buf = buf;
            g_sends[i].count = count;
            g_sends[i].dtype = datatype;
            *request = i + 1;
            return MPI_SUCCESS;
        }
    }
    abort();
}

int MPI_Send(const void *buf, int count, MPI_Datatype datatype, int dest, int tag, MPI_Comm comm) {
    MPI_Request req = 0;
    return MPI_Issend(buf, count, datatype, dest, tag, comm, &req);
}

int MPI_Ssend(const void *buf, int count, MPI_Datatype datatype, int dest, int tag, MPI_Comm comm) {
    return MPI_Send(buf, count, datatype, dest, tag, comm);
}

int MPI_Recv(void *buf, int count, MPI_Datatype datatype, int source, int tag, MPI_Comm comm, MPI_Status *status) {
    (void)source;
    (void)comm;
    for (int i = 0; i < MAX_MSGS; i++) {
        if (g_sends[i].active && g_sends[i].tag == tag) {
            size_t n = (size_t)count * type_size(datatype);
            size_t sn = (size_t)g_sends[i].count * type_size(g_sends[i].dtype);
            if (sn < n) n = sn;
            if (buf != g_sends[i].buf && n > 0) {
                memcpy(buf, g_sends[i].buf, n);
            }
            g_sends[i].active = 0;
            if (status) {
                status->MPI_SOURCE = 0;
                status->MPI_TAG = tag;
                status->MPI_ERROR = MPI_SUCCESS;
                status->bytes = n;
            }
            return MPI_SUCCESS;
        }
    }
    abort();
}

int MPI_Irecv(void *buf, int count, MPI_Datatype datatype, int source, int tag, MPI_Comm comm, MPI_Request *request) {
    MPI_Status st;
    int rc = MPI_Recv(buf, count, datatype, source, tag, comm, &st);
    *request = 0;
    return rc;
}

int MPI_Sendrecv(const void *sendbuf, int sendcount, MPI_Datatype sendtype, int dest, int sendtag, void *recvbuf, int recvcount, MPI_Datatype recvtype, int source, int recvtag, MPI_Comm comm, MPI_Status *status) {
    (void)dest;
    (void)source;
    (void)comm;
    if (sendtag != recvtag) abort();
    size_t n = (size_t)recvcount * type_size(recvtype);
    size_t sn = (size_t)sendcount * type_size(sendtype);
    if (sn < n) n = sn;
    if (recvbuf != sendbuf && n > 0) {
        memmove(recvbuf, sendbuf, n);
    }
    if (status) {
        status->MPI_SOURCE = 0;
        status->MPI_TAG = recvtag;
        status->MPI_ERROR = MPI_SUCCESS;
        status->bytes = n;
    }
    return MPI_SUCCESS;
}

int MPI_Wait(MPI_Request *request, MPI_Status *status) {
    if (*request > 0 && *request <= MAX_MSGS) {
        g_sends[*request - 1].active = 0;
    }
    *request = 0;
    if (status) {
        status->MPI_SOURCE = 0;
        status->MPI_TAG = 0;
        status->MPI_ERROR = MPI_SUCCESS;
    }
    return MPI_SUCCESS;
}

int MPI_Iprobe(int source, int tag, MPI_Comm comm, int *flag, MPI_Status *status) {
    (void)source;
    (void)comm;
    for (int i = 0; i < MAX_MSGS; i++) {
        if (g_sends[i].active && g_sends[i].tag == tag) {
            *flag = 1;
            if (status) {
                status->MPI_SOURCE = 0;
                status->MPI_TAG = tag;
                status->MPI_ERROR = MPI_SUCCESS;
            }
            return MPI_SUCCESS;
        }
    }
    *flag = 0;
    return MPI_SUCCESS;
}
EOF

gcc -O2 -Wall -Wextra -Werror -c "$work/mpi-stub/mpi_serial.c" -o "$work/mpi-stub/mpi_serial.o"
ar rcs "$work/mpi-stub/libmpi.a" "$work/mpi-stub/mpi_serial.o"

# 3. Build static xhpl against libopenblas.a and libmpi.a.
cd "$work/hpl-$HPL_VERSION"
cat >Make.static <<EOF
SHELL        = /bin/sh
CD           = cd
CP           = cp
LN_S         = ln -s
MKDIR        = mkdir
RM           = /bin/rm -f
TOUCH        = touch
ARCH         = static
TOPdir       = $work/hpl-$HPL_VERSION
INCdir       = \$(TOPdir)/include
BINdir       = \$(TOPdir)/bin/\$(ARCH)
LIBdir       = \$(TOPdir)/lib/\$(ARCH)
HPLlib       = \$(LIBdir)/libhpl.a
MPdir        = $work/mpi-stub
MPinc        = -I\$(MPdir)
MPlib        = \$(MPdir)/libmpi.a
LAdir        = $work/openblas
LAinc        = -I\$(LAdir)/include
LAlib        = \$(LAdir)/lib/libopenblas.a
F2CDEFS      = -DAdd_ -DF77_INTEGER=int -DStringSunStyle
HPL_INCLUDES = -I\$(INCdir) -I\$(INCdir)/\$(ARCH) \$(LAinc) \$(MPinc)
HPL_LIBS     = \$(HPLlib) \$(LAlib) \$(MPlib)
HPL_OPTS     = -DHPL_CALL_CBLAS
HPL_DEFS     = \$(F2CDEFS) \$(HPL_OPTS) \$(HPL_INCLUDES)
CC           = gcc
CCNOOPT      = \$(HPL_DEFS)
CCFLAGS      = \$(HPL_DEFS) -O3 -fopenmp
LINKER       = gcc
LINKFLAGS    = \$(CCFLAGS) -static
ARCHIVER     = ar
ARFLAGS      = r
RANLIB       = ranlib
EOF

make arch=static >/dev/null
xhpl="$work/hpl-$HPL_VERSION/bin/static/xhpl"
[ -f "$xhpl" ] || fail "xhpl was not built"
strip "$xhpl"

if readelf -l "$xhpl" | grep -q 'program interpreter'; then
    fail "xhpl is dynamically linked"
fi

# 4. Run a residual validation check on a 512x512 dense system with 2 OpenMP threads.
mkdir "$work/check"
cat >"$work/check/HPL.dat" <<'EOF'
HPLinpack benchmark input file
Innovative Computing Laboratory, University of Tennessee
HPL.out      output file name (if any)
6            device out (6=stdout,7=stderr,file)
1            # of problems sizes (N)
512          Ns
1            # of NBs
64           NBs
0            PMAP process mapping (0=Row-,1=Column-major)
1            # of process grids (P x Q)
1            Ps
1            Qs
16.0         threshold
1            # of panel fact
2            PFACTs (0=left, 1=Crout, 2=Right)
1            # of recursive stopping criterium
4            NBMINs (>= 1)
1            # of panels in recursion
2            NDIVs
1            # of recursive panel fact.
1            RFACTs (0=left, 1=Crout, 2=Right)
1            # of broadcast
1            BCASTs (0=1rg,1=1rM,2=2rg,3=2rM,4=Lng,5=LnM)
1            # of lookahead depth
1            DEPTHs (>=0)
2            SWAP (0=bin-exch,1=long,2=mix)
64           swapping threshold
0            L1 in (0=transposed,1=no-transposed) form
0            U  in (0=transposed,1=no-transposed) form
1            Equilibration (0=no,1=yes)
8            memory alignment in double (> 0)
EOF

check_out=$(cd "$work/check" && OMP_NUM_THREADS=2 OPENBLAS_NUM_THREADS=2 "$xhpl" 2>&1) ||
    fail "xhpl check run failed: $check_out"
case $check_out in
*"1 tests completed and passed residual checks"*) ;;
*) fail "xhpl did not pass residual check: $check_out" ;;
esac

cp "$xhpl" "$out/hpl-$arch"
cp "$0" "$out/build-hpl.sh"
cp "$work/hpl-$HPL_VERSION.tar.gz" "$work/$OPENBLAS_ARCHIVE" "$out/"
pack "$out/hpl-$HPL_VERSION-mpi-stub.tar.gz" -C "$work" mpi-stub/mpi.h mpi-stub/mpi_serial.c
cp "$work/hpl-$HPL_VERSION/COPYRIGHT" "$out/COPYRIGHT-hpl"
cp "$work/OpenBLAS-$OPENBLAS_VERSION/LICENSE" "$out/LICENSE-openblas"
cp "$work/musl-$MUSL_VERSION/COPYRIGHT" "$out/COPYRIGHT-musl"
rm -rf "$work"
echo "build-hpl: built $out/hpl-$arch (HPL $HPL_VERSION + OpenBLAS $OPENBLAS_VERSION)"
