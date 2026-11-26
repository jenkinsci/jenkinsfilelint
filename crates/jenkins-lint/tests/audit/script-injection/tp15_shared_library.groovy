def call(Map config = [:]) {
    sh "git push origin HEAD:${env.CHANGE_BRANCH}"
    sh "release --version ${params.VERSION}"
}
