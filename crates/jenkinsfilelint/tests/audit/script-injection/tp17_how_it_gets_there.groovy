node {
    def out = sh(script: "git log origin/${env.CHANGE_BRANCH}", returnStdout: true)
    sh 'echo ' + env.CHANGE_TITLE
    sh params.COMMAND
    def cmd = 'echo '
    cmd += env.CHANGE_TITLE
    sh cmd
}
