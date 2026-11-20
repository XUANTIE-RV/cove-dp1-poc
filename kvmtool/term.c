/*
 * Modified downstream for CoVE DP-1 on 2026-08-23.
 * See kvmtool/PROVENANCE.md for source lineage details.
 */

#include <poll.h>
#include <stdbool.h>
#include <termios.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/uio.h>
#include <signal.h>
#include <pty.h>
#include <utmp.h>

#include "kvm/read-write.h"
#include "kvm/term.h"
#include "kvm/util.h"
#include "kvm/kvm.h"
#include "kvm/kvm-cpu.h"

#define TERM_FD_IN      0
#define TERM_FD_OUT     1

static struct termios	orig_term;

/* The SIGTERM handler needs the guest to ask it to stop; term_init is handed one. */
static struct kvm	*term_kvm;
static volatile sig_atomic_t term_stop_requested;

static int term_fds[TERM_MAX_DEVS][2];

static pthread_t term_poll_thread;

/* ctrl-a is used for escape */
#define term_escape_char	0x01

int term_getc(struct kvm *kvm, int term)
{
	static bool term_got_escape = false;
	unsigned char c;

	if (read_in_full(term_fds[term][TERM_FD_IN], &c, 1) < 0)
		return -1;

	if (term_got_escape) {
		term_got_escape = false;
		if (c == 'x')
			kvm__reboot(kvm);
		if (c == term_escape_char)
			return c;
	}

	if (c == term_escape_char) {
		term_got_escape = true;
		return -1;
	}

	return c;
}

int term_putc(char *addr, int cnt, int term)
{
	int ret;
	int num_remaining = cnt;

	while (num_remaining) {
		ret = write(term_fds[term][TERM_FD_OUT], addr, num_remaining);
		if (ret < 0)
			return cnt - num_remaining;
		num_remaining -= ret;
		addr += ret;
	}

	return cnt;
}

int term_getc_iov(struct kvm *kvm, struct iovec *iov, int iovcnt, int term)
{
	int c;

	c = term_getc(kvm, term);

	if (c < 0)
		return 0;

	*((char *)iov[TERM_FD_IN].iov_base)	= (char)c;

	return sizeof(char);
}

int term_putc_iov(struct iovec *iov, int iovcnt, int term)
{
	return writev(term_fds[term][TERM_FD_OUT], iov, iovcnt);
}

bool term_readable(int term)
{
	struct pollfd pollfd = (struct pollfd) {
		.fd	= term_fds[term][TERM_FD_IN],
		.events	= POLLIN,
		.revents = 0,
	};
	int err;

	err = poll(&pollfd, 1, 0);
	return (err > 0 && (pollfd.revents & POLLIN));
}

static void *term_poll_thread_loop(void *param)
{
	struct pollfd fds[TERM_MAX_DEVS];
	struct kvm *kvm = (struct kvm *) param;
	int i;

	kvm__set_thread_name("term-poll");

	for (i = 0; i < TERM_MAX_DEVS; i++) {
		fds[i].fd = term_fds[i][TERM_FD_IN];
		fds[i].events = POLLIN;
		fds[i].revents = 0;
	}

	while (1) {
		/* Poll with infinite timeout */
		if(poll(fds, TERM_MAX_DEVS, -1) < 1)
			break;
		kvm__arch_read_term(kvm);
	}

	die("term_poll_thread_loop: error polling device fds %d\n", errno);
	return NULL;
}

static void term_cleanup(void)
{
	int i;

	for (i = 0; i < TERM_MAX_DEVS; i++)
		tcsetattr(term_fds[i][TERM_FD_IN], TCSANOW, &orig_term);
}

static void term_sig_cleanup(int sig)
{
	term_cleanup();

	/* This used to end here: restore the tty, restore the default disposition,
	 * re-raise, die. Nothing on the exit_list ran -- no device teardown, no disk
	 * flush -- because raising a fatal signal does not unwind anything; the
	 * ordering was left to the kernel closing our descriptors for us.
	 *
	 * Ask vCPU 0 to leave KVM_RUN instead. That is the same request `lkvm stop`
	 * and ctrl-a x make, and it lets the normal path run: vCPU 0 returns, the
	 * main thread joins it, and init_list__exit tears the devices down.
	 *
	 * Only ask when there is something to ask. kvm__reboot returns void and
	 * no-ops when vCPU 0 has no thread yet, so the same liveness test has to be
	 * repeated here -- without it a SIGTERM arriving before the guest runs would
	 * be swallowed and the process would survive a signal that used to kill it.
	 *
	 * The claim is an exchange, not a load and a store. SIGTERM is unblocked on
	 * every thread, and sig_atomic_t is only atomic against signals delivered to
	 * the same thread, so two of them landing on two threads could otherwise both
	 * read zero and both ask.
	 */
	if (term_kvm && term_kvm->cpus && term_kvm->cpus[0] &&
	    term_kvm->cpus[0]->thread != 0 &&
	    __atomic_exchange_n(&term_stop_requested, 1, __ATOMIC_SEQ_CST) == 0) {
		kvm__reboot(term_kvm);
		return;
	}

	/* Second signal, or no running vCPU to ask. A vCPU that never comes back out
	 * of KVM_RUN must not leave SIGTERM unable to kill this process. */
	signal(sig, SIG_DFL);
	raise(sig);
}

static void term_set_tty(int term)
{
	struct termios orig_term;
	int master, slave;
	char new_pty[PATH_MAX];

	if (tcgetattr(STDIN_FILENO, &orig_term) < 0)
		die("unable to save initial standard input settings");

	orig_term.c_lflag &= ~(ICANON | ECHO | ISIG);

	if (openpty(&master, &slave, new_pty, &orig_term, NULL) < 0)
		return;

	close(slave);

	pr_info("Assigned terminal %d to pty %s\n", term, new_pty);

	term_fds[term][TERM_FD_IN] = term_fds[term][TERM_FD_OUT] = master;
}

int tty_parser(const struct option *opt, const char *arg, int unset)
{
	int tty = atoi(arg);

	term_set_tty(tty);

	return 0;
}

static int term_init(struct kvm *kvm)
{
	struct termios term;
	int i, r;

	for (i = 0; i < TERM_MAX_DEVS; i++)
		if (term_fds[i][TERM_FD_IN] == 0) {
			term_fds[i][TERM_FD_IN] = STDIN_FILENO;
			term_fds[i][TERM_FD_OUT] = STDOUT_FILENO;
		}

	if (!isatty(STDIN_FILENO) || !isatty(STDOUT_FILENO))
		return 0;

	r = tcgetattr(STDIN_FILENO, &orig_term);
	if (r < 0) {
		pr_warning("unable to save initial standard input settings");
		return r;
	}


	term = orig_term;
	term.c_iflag &= ~(ICRNL);
	term.c_lflag &= ~(ICANON | ECHO | ISIG);
	tcsetattr(STDIN_FILENO, TCSANOW, &term);


	/* Use our own blocking thread to read stdin, don't require a tick */
	if(pthread_create(&term_poll_thread, NULL, term_poll_thread_loop,kvm))
		die("Unable to create console input poll thread\n");

	term_kvm = kvm;
	signal(SIGTERM, term_sig_cleanup);
	atexit(term_cleanup);

	return 0;
}
dev_init(term_init);

static int term_exit(struct kvm *kvm)
{
	return 0;
}
dev_exit(term_exit);
