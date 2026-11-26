pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script { take 2.pills of chloroquinine after 6.hours }
            }
        }
    }
}
