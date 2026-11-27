pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sleep((99999999999999999999))
            }
        }
    }
}
