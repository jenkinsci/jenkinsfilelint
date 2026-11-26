pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                bat "echo ${CHANGE_TITLE}"
            }
        }
    }
}
