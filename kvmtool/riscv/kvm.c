/*
 * Modified downstream for CoVE DP-1 on 2024-11-20, 2024-11-27, 2025-01-14, 2026-08-10, 2026-08-24.
 * See kvmtool/PROVENANCE.md for source lineage details.
 */

#include "kvm/kvm.h"
#include "kvm/util.h"
#include "kvm/8250-serial.h"
#include "kvm/virtio-console.h"
#include "kvm/fdt.h"

#include <linux/kernel.h>
#include <linux/kvm.h>
#include <linux/sizes.h>
#include <linux/virtio_config.h>
#include <unistd.h>

struct kvm_ext kvm_req_ext[] = {
	{ DEFINE_KVM_EXT(KVM_CAP_ONE_REG) },
	{ 0, 0 },
};

void kvm_cove_measure_region(struct kvm *kvm, unsigned long uaddr,
			      unsigned long gpa, unsigned long rsize)
{
	int ret;

	if (!kvm->cfg.arch.cove_vm)
		return;

	struct kvm_riscv_cove_measure_region mr = {
		.user_addr = uaddr,
		.gpa = gpa,
		.size = rsize,
	};

	ret = ioctl(kvm->vm_fd, KVM_RISCV_COVE_MEASURE_REGION, &mr);
	if (ret < 0) {
		ret = -errno;
		die("Setting measure region failed for CoVE VM\n");
	}
}

u64 kvm__arch_default_ram_address(void)
{
	return RISCV_RAM;
}

int kvm__get_vm_type(struct kvm *kvm)
{
	if (kvm->cfg.arch.cove_vm) {
		if (__riscv_xlen == 64)
			return KVM_VM_TYPE_RISCV_COVE;
		else
			die("CoVE VM is not supported in RV32\n");
	} else {
		return KVM_VM_TYPE;
	}
}

void kvm__arch_validate_cfg(struct kvm *kvm)
{
	long online;
	int effective_nrcpus, last;

	if (!kvm->cfg.arch.cove_vm)
		return;

	/*
	 * The pinning declared in riscv/kvm-cpu.c reads
	 *   target_cpu = vcpu->cpu_id + cpu_offset
	 * and calls sched_setaffinity() to that single CPU before every
	 * KVM_RUN. If any target_cpu is off the online range, the affinity
	 * call fails and lkvm dies via die_perror("sched_setaffinity") --
	 * a message with no clue that the reason is a bad --cpu-offset.
	 * Reject the value here, before any vCPU is created, and name what
	 * would have happened.
	 */
	if (kvm->cfg.arch.cpu_offset < 0)
		die("--cpu-offset must not be negative (got %d)",
		    kvm->cfg.arch.cpu_offset);

	online = sysconf(_SC_NPROCESSORS_ONLN);
	if (online <= 0)
		return; /* cannot check; leave it to sched_setaffinity */

	/*
	 * This validator runs before builtin-run.c fills kvm->nrcpus from
	 * kvm->cfg.nrcpus (which itself defaults to nr_online_cpus when the
	 * user did not pass -c). Reading kvm->nrcpus here would see 0 and
	 * turn the range check into "cpu_offset < online" alone -- fail-open
	 * for every offset that lands inside the online range regardless of
	 * how many vCPUs it consumes. Read cfg.nrcpus and mirror the same
	 * default the builtin-run path will apply.
	 */
	effective_nrcpus = kvm->cfg.nrcpus;
	if (effective_nrcpus == 0)
		effective_nrcpus = online;

	last = kvm->cfg.arch.cpu_offset + effective_nrcpus - 1;
	if (last >= online)
		die("--cpu-offset %d + %d vCPUs would pin vCPU %d to CPU %d, but only %ld CPUs are online",
		    kvm->cfg.arch.cpu_offset, effective_nrcpus,
		    effective_nrcpus - 1, last, online);
}

bool kvm__arch_cpu_supports_vm(void)
{
	/* The KVM capability check is enough. */
	return true;
}

void kvm__init_ram(struct kvm *kvm)
{
	int err;
	u64 phys_start, phys_size;
	void *host_mem;

	phys_start	= RISCV_RAM;
	phys_size	= kvm->ram_size;
	host_mem	= kvm->ram_start;

	err = kvm__register_ram(kvm, phys_start, phys_size, host_mem);
	if (err)
		die("Failed to register %lld bytes of memory at physical "
		    "address 0x%llx [err %d]", phys_size, phys_start, err);

	kvm->arch.memory_guest_start = phys_start;
}

void kvm__arch_delete_ram(struct kvm *kvm)
{
	munmap(kvm->arch.ram_alloc_start, kvm->arch.ram_alloc_size);
}

void kvm__arch_read_term(struct kvm *kvm)
{
	serial8250__update_consoles(kvm);
	virtio_console__inject_interrupt(kvm);
}

void kvm__arch_set_cmdline(char *cmdline, bool video)
{
}

#if __riscv_xlen == 64
#define HUGEPAGE_SIZE	SZ_2M
#else
#define HUGEPAGE_SIZE	SZ_4M
#endif

void kvm__arch_init(struct kvm *kvm)
{
	/*
	 * Allocate guest memory. We must align our buffer to 64K to
	 * correlate with the maximum guest page size for virtio-mmio.
	 * If using THP, then our minimal alignment becomes hugepage
	 * size. The hugepage size is always greater than 64K, so
	 * let's go with that.
	 */
	kvm->ram_size = min(kvm->cfg.ram_size, (u64)RISCV_MAX_MEMORY(kvm));
	kvm->arch.ram_alloc_size = kvm->ram_size;
	if (!kvm->cfg.hugetlbfs_path)
		kvm->arch.ram_alloc_size += HUGEPAGE_SIZE;
	kvm->arch.ram_alloc_start = mmap_anon_or_hugetlbfs(kvm,
						kvm->cfg.hugetlbfs_path,
						kvm->arch.ram_alloc_size);

	if (kvm->arch.ram_alloc_start == MAP_FAILED)
		die("Failed to map %lld bytes for guest memory (%d)",
		    kvm->arch.ram_alloc_size, errno);

	kvm->ram_start = (void *)ALIGN((unsigned long)kvm->arch.ram_alloc_start,
					SZ_2M);

	madvise(kvm->arch.ram_alloc_start, kvm->arch.ram_alloc_size,
		MADV_MERGEABLE);

	madvise(kvm->arch.ram_alloc_start, kvm->arch.ram_alloc_size,
		MADV_HUGEPAGE);

	riscv__irqchip_create(kvm);
}

#define FDT_ALIGN	SZ_4M
/*
 * The initrd only needs 8-byte alignment to boot, but a CoVE VM also has it
 * measured, and KVM_RISCV_COVE_MEASURE_REGION rejects a gpa or host address
 * that is not page aligned. Place it on a page boundary so the same path works
 * for both; an ext4 rootfs passed with -d never exercised this.
 */
#define INITRD_ALIGN	SZ_4K
bool kvm__arch_load_kernel_image(struct kvm *kvm, int fd_kernel, int fd_initrd,
				 const char *kernel_cmdline)
{
	void *pos, *kernel_end, *limit;
	unsigned long guest_addr, kernel_offset;
	ssize_t file_size;

	/*
	 * Linux requires the initrd and dtb to be mapped inside lowmem,
	 * so we can't just place them at the top of memory.
	 */
	limit = kvm->ram_start + min(kvm->ram_size, (u64)SZ_1G) - 1;

#if __riscv_xlen == 64
	/* Linux expects to be booted at 2M boundary for RV64 */
	kernel_offset = 0x200000;
#else
	/* Linux expects to be booted at 4M boundary for RV32 */
	kernel_offset = 0x400000;
#endif

	pos = kvm->ram_start + kernel_offset;
	kvm->arch.kern_guest_start = host_to_guest_flat(kvm, pos);
	file_size = read_file(fd_kernel, pos, limit - pos);
	if (file_size < 0) {
		if (errno == ENOMEM)
			die("kernel image too big to fit in guest memory.");

		die_perror("kernel read");
	}
	kernel_end = pos + file_size;
	pr_debug("Loaded kernel to 0x%llx (%zd bytes)",
		 kvm->arch.kern_guest_start, file_size);

	kvm_cove_measure_region(kvm, (unsigned long)pos, kvm->arch.kern_guest_start,
			       file_size);
	/* Place FDT just after kernel at FDT_ALIGN address */
	pos = kernel_end + FDT_ALIGN;
	guest_addr = ALIGN(host_to_guest_flat(kvm, pos), FDT_ALIGN);
	pos = guest_flat_to_host(kvm, guest_addr);
	if (pos < kernel_end)
		die("fdt overlaps with kernel image.");

	kvm->arch.dtb_guest_start = guest_addr;
	pr_debug("Placing fdt at 0x%llx - 0x%llx",
		 kvm->arch.dtb_guest_start,
		 host_to_guest_flat(kvm, limit));

	/* ... and finally the initrd, if we have one. */
	if (fd_initrd != -1) {
		struct stat sb;
		unsigned long initrd_start;

		if (fstat(fd_initrd, &sb))
			die_perror("fstat");

		pos = limit - (sb.st_size + INITRD_ALIGN);
		guest_addr = ALIGN(host_to_guest_flat(kvm, pos), INITRD_ALIGN);
		pos = guest_flat_to_host(kvm, guest_addr);
		if (pos < kernel_end)
			die("initrd overlaps with kernel image.");

		initrd_start = guest_addr;
		file_size = read_file(fd_initrd, pos, limit - pos);
		if (file_size == -1) {
			if (errno == ENOMEM)
				die("initrd too big to fit in guest memory.");

			die_perror("initrd read");
		}

		kvm->arch.initrd_guest_start = initrd_start;
		kvm->arch.initrd_size = file_size;
		pr_debug("Loaded initrd to 0x%llx (%llu bytes)",
			 kvm->arch.initrd_guest_start,
			 kvm->arch.initrd_size);
		kvm_cove_measure_region(kvm, (unsigned long)pos, initrd_start,
				       file_size);
	} else {
		kvm->arch.initrd_size = 0;
	}

	return true;
}

bool kvm__load_firmware(struct kvm *kvm, const char *firmware_filename)
{
	/* TODO: Firmware loading to be supported later. */
	return false;
}

int kvm__arch_setup_firmware(struct kvm *kvm)
{
	return 0;
}

u64 kvm__arch_get_virtio_host_features(struct kvm *kvm)
{
	u64 features = 0;

	/* CoVE VMs mandate VIRTIO_F_ACCESS_PLATFORM feature to force use of
	 * SWIOTLB bounce buffers through DMA API. Without this device probe
	 * will fail for CoVE VMs.
	 */
	if (kvm->cfg.arch.cove_vm)
		features |= (1ULL << VIRTIO_F_ACCESS_PLATFORM);

	return features;
}
