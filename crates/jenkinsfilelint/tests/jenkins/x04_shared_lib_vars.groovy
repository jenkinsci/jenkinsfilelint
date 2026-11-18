def call(Map config = [:]) {
    node {
        echo "Deploying ${config.app}"
    }
}
