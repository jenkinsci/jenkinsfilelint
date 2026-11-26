pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                powershell "git checkout origin/${CHANGE_BRANCH}"
            }
        }
    }
}
