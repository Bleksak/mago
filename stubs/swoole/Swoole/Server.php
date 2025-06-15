<?php

declare(strict_types=1);

namespace Swoole;

use Swoole\Coroutine;
use Swoole\Event;
use Swoole\Process;

class Server
{
    public $setting;
    public $connections;
    public $host = '';
    public $port = 0;
    public $type = 0;
    public $mode = 0;
    public $ports;
    public $master_pid = 0;
    public $manager_pid = 0;
    public $worker_id = -1;
    public $taskworker = false;
    public $worker_pid = 0;
    public $stats_timer;

    /**
     * @var Coroutine\Http\Server
     */
    public $admin_server;

    /**
     * @var callable
     */
    private $onStart;

    /**
     * @var callable
     */
    private $onBeforeShutdown;

    /**
     * @var callable
     */
    private $onShutdown;

    /**
     * @var callable
     */
    private $onWorkerStart;

    /**
     * @var callable
     */
    private $onWorkerStop;

    /**
     * @var callable
     */
    private $onBeforeReload;

    /**
     * @var callable
     */
    private $onAfterReload;

    /**
     * @var callable
     */
    private $onWorkerExit;

    /**
     * @var callable
     */
    private $onWorkerError;

    /**
     * @var callable
     */
    private $onTask;

    /**
     * @var callable
     */
    private $onFinish;

    /**
     * @var callable
     */
    private $onManagerStart;

    /**
     * @var callable
     */
    private $onManagerStop;

    /**
     * @var callable
     */
    private $onPipeMessage;

    public function __construct($host, $port = null, $mode = null, $sock_type = null) {}

    public function __destruct()
    {
    }

    public function listen($host, $port, $sock_type)
    {
    }

    public function addlistener($host, $port, $sock_type)
    {
    }

    public function on($event_name, callable $callback)
    {
    }

    public function getCallback($event_name)
    {
    }

    /**
     * @return mixed
     */
    public function set(array $settings)
    {
    }

    /**
     * @return mixed
     */
    public function start()
    {
    }

    /**
     * @param mixed $fd
     * @param mixed $send_data
     * @param mixed|null $server_socket
     * @return mixed
     */
    public function send($fd, $send_data, $server_socket = null)
    {
    }

    /**
     * @param mixed $ip
     * @param mixed $port
     * @param mixed $send_data
     * @param mixed|null $server_socket
     * @return mixed
     */
    public function sendto($ip, $port, $send_data, $server_socket = null)
    {
    }

    /**
     * @param mixed $conn_fd
     * @param mixed $send_data
     * @return mixed
     */
    public function sendwait($conn_fd, $send_data)
    {
    }

    /**
     * @param mixed $fd
     * @return mixed
     */
    public function exists($fd)
    {
    }

    /**
     * @param mixed $fd
     * @return mixed
     */
    public function exist($fd)
    {
    }

    /**
     * @param mixed $fd
     * @param mixed|null $is_protected
     * @return mixed
     */
    public function protect($fd, $is_protected = null)
    {
    }

    /**
     * @param mixed $conn_fd
     * @param mixed $filename
     * @param mixed|null $offset
     * @param mixed|null $length
     * @return mixed
     */
    public function sendfile($conn_fd, $filename, $offset = null, $length = null)
    {
    }

    /**
     * @param mixed $fd
     * @param mixed|null $reset
     * @return mixed
     */
    public function close($fd, $reset = null)
    {
    }

    /**
     * @param mixed $fd
     * @return mixed
     */
    public function confirm($fd)
    {
    }

    /**
     * @param mixed $fd
     * @return mixed
     */
    public function pause($fd)
    {
    }

    /**
     * @param mixed $fd
     * @return mixed
     */
    public function resume($fd)
    {
    }

    /**
     * @param mixed $data
     * @param mixed|null $worker_id
     * @return mixed
     */
    public function task($data, $worker_id = null, null|callable $finish_callback = null)
    {
    }

    /**
     * @param mixed $data
     * @param mixed|null $timeout
     * @param mixed|null $worker_id
     * @return mixed
     */
    public function taskwait($data, $timeout = null, $worker_id = null)
    {
    }

    /**
     * @param mixed|null $timeout
     * @return mixed
     */
    public function taskWaitMulti(array $tasks, $timeout = null)
    {
    }

    /**
     * @param mixed|null $timeout
     * @return mixed
     */
    public function taskCo(array $tasks, $timeout = null)
    {
    }

    /**
     * @param mixed $data
     * @return mixed
     */
    public function finish($data)
    {
    }

    /**
     * @return mixed
     */
    public function reload()
    {
    }

    /**
     * @return mixed
     */
    public function shutdown()
    {
    }

    /**
     * @param mixed|null $worker_id
     * @return mixed
     */
    public function stop($worker_id = null)
    {
    }

    /**
     * @return mixed
     */
    public function getLastError()
    {
    }

    /**
     * @param mixed $reactor_id
     * @return mixed
     */
    public function heartbeat($reactor_id)
    {
    }

    /**
     * @param mixed $fd
     * @param mixed|null $reactor_id
     * @return mixed
     */
    public function getClientInfo($fd, $reactor_id = null)
    {
    }

    /**
     * @param mixed $start_fd
     * @param mixed|null $find_count
     * @return mixed
     */
    public function getClientList($start_fd, $find_count = null)
    {
    }

    /**
     * Get the ID of current worker (either an event worker or a task worker).
     *
     * @return int|false Returns the ID of current worker. Returns false if not called within a worker process (either
     *                   an event worker process or a task worker process).
     */
    public function getWorkerId()
    {
    }

    /**
     * Get the process ID of a given worker process (specified by a worker ID).
     *
     * @return int|false Returns the process ID of a given worker process (specified by a worker ID). If the worker ID
     *                   is a negative integer or not passed in, returns the process ID of current worker process.
     *                   Returns false if something wrong happens (e.g., the worker process doesn't exist, or an invalid
     *                   worker ID specified.).
     */
    public function getWorkerPid(int $worker_id = -1)
    {
    }

    /**
     * @param mixed|null $worker_id
     * @return mixed
     */
    public function getWorkerStatus($worker_id = null)
    {
    }

    public function command(string $name, int $process_id, int $process_type, $data, bool $json_decode = true)
    {
    }

    /**
     * @return bool
     */
    public function addCommand(string $name, int $accepted_process_types, callable $callback)
    {
    }

    public function getManagerPid()
    {
    }

    public function getMasterPid()
    {
    }

    public function connection_info($fd, $reactor_id = null)
    {
    }

    public function connection_list($start_fd, $find_count = null)
    {
    }

    public function sendMessage($message, $dst_worker_id)
    {
    }

    /**
     * @return int|false
     */
    public function addProcess(Process $process)
    {
    }

    public function stats()
    {
    }

    public function getSocket($port = null)
    {
    }

    public function bind($fd, $uid)
    {
    }

    /**
     * @return int
     */
    public function after(int $ms, callable $callback, ...$params)
    {
    }

    /**
     * @return int
     */
    public function tick(int $ms, callable $callback, ...$params)
    {
    }

    /**
     * @return bool
     */
    public function clearTimer(int $timer_id)
    {
    }

    /**
     * @return true
     */
    public function defer(callable $callback)
    {
    }
}
