pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "find -name "${n}" | egrep ."
            }
        }
    }
}
