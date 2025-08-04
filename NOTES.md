# notes on integrating UDT

The recommended epoll API is...pretty bad. It doesn't actually use any sort of poller normally, it's just a slightly more efficient variant of the UDT::select/Ex functions which use mutex/condvar shenanigans. epoll/select are *actually* used with a zero timeout and checked periodically after the timer expires for checking UDT sockets. So if UDT epoll is used with *only* system sockets, it effectively busy waits until they're ready. NOT GOOD.

The select API is *really slow* for some reason. But because of the ungodly amount of busy-waiting that goes on in UDT epoll, it seems to be the better alternative.