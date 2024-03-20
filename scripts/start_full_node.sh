# includes
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
. $SCRIPT_DIR/utils/shared.sh

function stop_full_node() {
    if [[ $DONE == 1 ]]; then
        # already terminated, this function is called
        # on SIGINT but also before exit
        return
    fi

    stop_all_geth
    stop_all_chains

    DONE=1
}

trap stop_full_node SIGINT

if [ -z $1 ]; then
    APP_ARGS[3]="--mine"
else
    APP_ARGS[3]="--mine --remote-bootnode $1"
fi


echo "Starting geth and chain"
start_geth 3
sleep 2
start_full_node_from_genesis 3

# wait for any job to terminate
wait "${CHAIN_PIDS[@]}"
stop_full_node
exit 1
