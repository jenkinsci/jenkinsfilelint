pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withEnv(["TITLE=${env.CHANGE_TITLE}"]) {
                    sh 'echo "$TITLE"'
                }
            }
        }
    }
}
