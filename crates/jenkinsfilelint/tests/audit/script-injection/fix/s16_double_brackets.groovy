pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "[[ ${CHANGE_BRANCH} == main ]] && make"
            }
        }
    }
}
