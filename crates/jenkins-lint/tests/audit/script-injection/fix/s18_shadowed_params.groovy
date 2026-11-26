def call(Map params = [:]) {
    sh "deploy --target ${params.target} --branch ${env.CHANGE_BRANCH}"
}
